use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::Config;
use crate::metrics::MetricBatch;

#[derive(Debug, Serialize)]
pub struct ServerRegistration {
    pub agent_id: String,
    pub hostname: String,
    pub agent_version: String,
    pub platform: String,
    pub arch: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerRegistrationResponse {
    pub server_id: String,
    pub status: String,
    pub message: Option<String>,
}

pub struct ApiClient {
    client: Client,
    endpoint: String,
    api_key: Option<String>,
}

impl ApiClient {
    pub fn new(config: &Config) -> Result<Self, ApiError> {
        let timeout = Duration::from_secs(config.get_api_timeout_seconds());
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| ApiError::ClientCreation(e.to_string()))?;

        Ok(Self {
            client,
            endpoint: config.api.endpoint.clone(),
            api_key: config.api.api_key.clone(),
        })
    }

    pub async fn send_metrics(&self, batch: &MetricBatch) -> Result<(), ApiError> {
        let url = format!("{}/api/v1/metrics", self.endpoint);

        let mut request = self.client
            .post(&url)
            .json(batch)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");

        // Add API key authentication if available
        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Request(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());

            return Err(ApiError::Response {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn register_server(&self, registration: &ServerRegistration) -> Result<ServerRegistrationResponse, ApiError> {
        let url = format!("{}/api/v1/servers", self.endpoint);

        let mut request = self.client
            .post(&url)
            .json(registration)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");

        // Add API key authentication if available
        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Request(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());

            return Err(ApiError::Response {
                status: status.as_u16(),
                body,
            });
        }

        let registration_response: ServerRegistrationResponse = response
            .json()
            .await
            .map_err(|e| ApiError::Parse(e.to_string()))?;

        Ok(registration_response)
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Failed to create HTTP client: {0}")]
    ClientCreation(String),
    #[error("Request failed: {0}")]
    Request(String),
    #[error("Failed to parse response: {0}")]
    Parse(String),
    #[error("API returned error status {status}: {body}")]
    Response { status: u16, body: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::metrics::{DiskMetric, MetricService};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn create_test_config(endpoint: &str) -> Config {
        Config::load_from_str(&format!(r#"
agent:
  id: "test-agent"
api:
  endpoint: "{}"
  timeout_seconds: 5
collection:
  interval_seconds: 60
  disk:
    enabled: true
"#, endpoint)).unwrap()
    }

    async fn create_test_config_with_api_key(endpoint: &str, api_key: &str) -> Config {
        Config::load_from_str(&format!(r#"
agent:
  id: "test-agent"
api:
  endpoint: "{}"
  timeout_seconds: 5
  api_key: "{}"
collection:
  interval_seconds: 60
  disk:
    enabled: true
"#, endpoint, api_key)).unwrap()
    }

    #[tokio::test]
    async fn test_api_client_creation() {
        let config = create_test_config("https://api.example.com").await;
        let client = ApiClient::new(&config).unwrap();
        assert_eq!(client.endpoint(), "https://api.example.com");
    }

    #[tokio::test]
    async fn test_send_metrics_success() {
        let mock_server = MockServer::start().await;
        let config = create_test_config(&mock_server.uri()).await;
        
        Mock::given(method("POST"))
            .and(path("/api/v1/metrics"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = ApiClient::new(&config).unwrap();
        let service = MetricService::new(&config);
        
        let metric = DiskMetric {
            timestamp: 1234567890,
            device: "/dev/sda1".to_string(),
            mount_point: "/".to_string(),
            total_space_bytes: 1000000,
            used_space_bytes: 500000,
            available_space_bytes: 500000,
            usage_percentage: 50.0,
        };

        let batch = service.create_batch(vec![metric], "test-agent", "test-host");
        let result = client.send_metrics(&batch).await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_metrics_server_error() {
        let mock_server = MockServer::start().await;
        let config = create_test_config(&mock_server.uri()).await;
        
        Mock::given(method("POST"))
            .and(path("/api/v1/metrics"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let client = ApiClient::new(&config).unwrap();
        let service = MetricService::new(&config);
        
        let metric = DiskMetric {
            timestamp: 1234567890,
            device: "/dev/sda1".to_string(),
            mount_point: "/".to_string(),
            total_space_bytes: 1000000,
            used_space_bytes: 500000,
            available_space_bytes: 500000,
            usage_percentage: 50.0,
        };

        let batch = service.create_batch(vec![metric], "test-agent", "test-host");
        let result = client.send_metrics(&batch).await;
        
        assert!(result.is_err());
        match result.unwrap_err() {
            ApiError::Response { status, body } => {
                assert_eq!(status, 500);
                assert_eq!(body, "Internal Server Error");
            }
            _ => panic!("Expected ApiError::Response"),
        }
    }

    #[tokio::test]
    async fn test_send_metrics_network_error() {
        let config = create_test_config("http://192.0.2.1:9999").await;
        let client = ApiClient::new(&config).unwrap();
        let service = MetricService::new(&config);
        
        let metric = DiskMetric {
            timestamp: 1234567890,
            device: "/dev/sda1".to_string(),
            mount_point: "/".to_string(),
            total_space_bytes: 1000000,
            used_space_bytes: 500000,
            available_space_bytes: 500000,
            usage_percentage: 50.0,
        };

        let batch = service.create_batch(vec![metric], "test-agent", "test-host");
        let result = client.send_metrics(&batch).await;
        
        assert!(result.is_err());
        match result.unwrap_err() {
            ApiError::Request(_) => {},
            _ => panic!("Expected ApiError::Request"),
        }
    }

    #[tokio::test]
    async fn test_server_registration_with_api_key() {
        let mock_server = MockServer::start().await;
        let config = create_test_config_with_api_key(&mock_server.uri(), "test-api-key").await;
        
        Mock::given(method("POST"))
            .and(path("/api/v1/servers"))
            .respond_with(ResponseTemplate::new(201).set_body_json(&serde_json::json!({
                "server_id": "srv_123456789",
                "status": "registered",
                "message": "Server registered successfully"
            })))
            .mount(&mock_server)
            .await;

        let client = ApiClient::new(&config).unwrap();
        
        let registration = ServerRegistration {
            agent_id: "test-agent".to_string(),
            hostname: "test-host".to_string(),
            agent_version: "0.1.0".to_string(),
            platform: "linux".to_string(),
            arch: "x86_64".to_string(),
        };

        let result = client.register_server(&registration).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert_eq!(response.server_id, "srv_123456789");
        assert_eq!(response.status, "registered");
        assert_eq!(response.message, Some("Server registered successfully".to_string()));
    }

    #[tokio::test]
    async fn test_server_registration_without_api_key() {
        let mock_server = MockServer::start().await;
        let config = create_test_config(&mock_server.uri()).await;
        
        Mock::given(method("POST"))
            .and(path("/api/v1/servers"))
            .respond_with(ResponseTemplate::new(201).set_body_json(&serde_json::json!({
                "server_id": "srv_123456789",
                "status": "registered"
            })))
            .mount(&mock_server)
            .await;

        let client = ApiClient::new(&config).unwrap();
        
        let registration = ServerRegistration {
            agent_id: "test-agent".to_string(),
            hostname: "test-host".to_string(),
            agent_version: "0.1.0".to_string(),
            platform: "linux".to_string(),
            arch: "x86_64".to_string(),
        };

        let result = client.register_server(&registration).await;
        assert!(result.is_ok());
    }
}
