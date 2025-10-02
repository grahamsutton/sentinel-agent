use std::collections::VecDeque;
use tokio::time::{Duration, interval};

use crate::client::{ApiClient, ApiError, ResourceRegistration};
use crate::config::Config;
use crate::metrics::{DiskMetric, MetricService};
use crate::state::ResourceState;

pub struct SentinelAgent {
    config: Config,
    hostname: String,
    api_client: ApiClient,
    metric_service: MetricService,
    buffer: VecDeque<DiskMetric>,
    resource_id: Option<String>,
}

impl SentinelAgent {
    pub fn new(config: Config) -> Result<Self, AgentError> {
        let hostname = config.get_hostname();
        let api_client =
            ApiClient::new(&config).map_err(|e| AgentError::Initialization(e.to_string()))?;
        let metric_service = MetricService::new(&config);

        Ok(Self {
            config,
            hostname,
            api_client,
            metric_service,
            buffer: VecDeque::new(),
            resource_id: None,
        })
    }

    fn add_to_buffer(&mut self, metrics: Vec<DiskMetric>) {
        self.buffer.extend(metrics);

        let max_size = self.config.get_batch_size();
        while self.buffer.len() > max_size {
            self.buffer.pop_front();
        }
    }

    async fn flush_buffer(&mut self) -> Result<(), AgentError> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        // Ensure we have a resource_id before sending metrics
        let resource_id = self.resource_id.as_ref()
            .ok_or_else(|| AgentError::Configuration("Resource not registered".to_string()))?;

        let metrics: Vec<DiskMetric> = self.buffer.drain(..).collect();
        let batch =
            self.metric_service
                .create_batch(metrics, resource_id, &self.hostname);

        self.api_client
            .send_metrics(&batch)
            .await
            .map_err(AgentError::Api)?;

        Ok(())
    }

    async fn collect_metrics(&self) -> Result<Vec<DiskMetric>, AgentError> {
        self.metric_service
            .collect_all_metrics()
            .map_err(|e| AgentError::MetricCollection(e.to_string()))
    }

    async fn register_resource(&mut self) -> Result<(), AgentError> {
        // Only register if API key is configured (indicating Operion platform integration)
        if self.config.api.api_key.is_none() {
            println!("API key not configured, skipping resource registration");
            return Ok(());
        }

        // Check if we already have a resource state
        match ResourceState::load() {
            Ok(Some(state)) => {
                println!("✅ Found existing resource registration");
                println!("   Resource ID: {}", state.resource_id);
                println!("   Registered at: {}", state.registered_at);
                self.resource_id = Some(state.resource_id);
                return Ok(());
            }
            Ok(None) => {
                println!("📝 No existing registration found, registering new resource...");
            }
            Err(e) => {
                eprintln!("⚠️  Error loading resource state: {}", e);
                eprintln!("   Will attempt to register new resource");
            }
        }

        // Perform new registration
        let registration = ResourceRegistration {
            hostname: self.hostname.clone(),
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        };

        match self.api_client.register_resource(&registration).await {
            Ok(response) => {
                println!("✅ Resource registered successfully");
                println!("   Resource ID: {}", response.resource_id);
                println!("   Status: {}", response.status);
                if let Some(message) = response.message {
                    println!("   Message: {}", message);
                }

                // Save the resource state
                let state = ResourceState::new(
                    response.resource_id.clone(),
                    env!("CARGO_PKG_VERSION").to_string(),
                );

                if let Err(e) = state.save() {
                    eprintln!("⚠️  Failed to save resource state: {}", e);
                    eprintln!("   Resource will be re-registered on next restart");
                } else {
                    println!("💾 Resource state saved to: {}", ResourceState::get_state_file_path().display());
                }

                self.resource_id = Some(response.resource_id);
                Ok(())
            }
            Err(e) => {
                eprintln!("⚠️  Resource registration failed: {}", e);
                eprintln!("   Agent will continue without registration");
                // Don't fail startup if registration fails - just log and continue
                Ok(())
            }
        }
    }

    pub async fn run(&mut self) -> Result<(), AgentError> {
        println!("Starting Operion Sentinel Agent...");
        println!("Hostname: {}", self.hostname);
        println!("API Endpoint: {}", self.api_client.endpoint());
        println!(
            "Collection interval: {} seconds",
            self.config.collection.interval_seconds
        );
        println!(
            "Flush interval: {} seconds",
            self.config.get_flush_interval_seconds()
        );

        // Register resource with Operion platform
        self.register_resource().await?;

        let mut collection_timer =
            interval(Duration::from_secs(self.config.collection.interval_seconds));
        let mut flush_timer = interval(Duration::from_secs(
            self.config.get_flush_interval_seconds(),
        ));

        loop {
            tokio::select! {
                _ = collection_timer.tick() => {
                    match self.collect_metrics().await {
                        Ok(metrics) => {
                            if !metrics.is_empty() {
                                println!("Collected {} disk metrics", metrics.len());
                                self.add_to_buffer(metrics);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to collect metrics: {}", e);
                        }
                    }
                }
                _ = flush_timer.tick() => {
                    match self.flush_buffer().await {
                        Ok(_) => {
                            if !self.buffer.is_empty() {
                                println!("Successfully flushed metrics buffer");
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to flush metrics: {}", e);
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Agent initialization failed: {0}")]
    Initialization(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("API error: {0}")]
    Api(#[from] ApiError),
    #[error("Metric collection error: {0}")]
    MetricCollection(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn create_test_config() -> Config {
        Config::load_from_str(r#"
agent:
  id: "test-agent"
api:
  endpoint: "https://api.example.com"
collection:
  interval_seconds: 60
  batch_size: 5
  disk:
    enabled: true
"#).unwrap()
    }

    #[test]
    fn test_agent_creation() {
        let config = create_test_config();
        let agent = SentinelAgent::new(config);
        assert!(agent.is_ok());
    }

    #[test]
    fn test_buffer_management() {
        let config = create_test_config();
        let mut agent = SentinelAgent::new(config).unwrap();

        let metrics = vec![
            DiskMetric {
                timestamp: 1234567890,
                device: "/dev/sda1".to_string(),
                mount_point: "/".to_string(),
                total_space_bytes: 1000000,
                used_space_bytes: 500000,
                available_space_bytes: 500000,
                usage_percentage: 50.0,
            };
            10
        ];

        agent.add_to_buffer(metrics);
        assert_eq!(agent.buffer.len(), 5);
    }

    #[tokio::test]
    async fn test_flush_empty_buffer() {
        let config = create_test_config();
        let mut agent = SentinelAgent::new(config).unwrap();
        let result = agent.flush_buffer().await;
        assert!(result.is_ok());
    }
}
