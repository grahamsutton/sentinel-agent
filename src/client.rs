use reqwest::Client;
use std::time::Duration;

use crate::config::Config;
use crate::metrics::MetricBatch;

pub struct ApiClient {
    client: Client,
    endpoint: String,
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
        })
    }

    pub async fn send_metrics(&self, batch: &MetricBatch) -> Result<(), ApiError> {
        let url = format!("{}/v1/metrics", self.endpoint);

        let response = self
            .client
            .post(&url)
            .json(batch)
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
            .and(path("/v1/metrics"))
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
            .and(path("/v1/metrics"))
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
}
