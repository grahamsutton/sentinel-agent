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
