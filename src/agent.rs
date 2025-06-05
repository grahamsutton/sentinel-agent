use std::collections::VecDeque;
use tokio::time::{Duration, interval};

use crate::client::{ApiClient, ApiError};
use crate::config::Config;
use crate::metrics::{DiskMetric, MetricService};

pub struct SentinelAgent {
    config: Config,
    hostname: String,
    api_client: ApiClient,
    metric_service: MetricService,
    buffer: VecDeque<DiskMetric>,
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

        let metrics: Vec<DiskMetric> = self.buffer.drain(..).collect();
        let batch =
            self.metric_service
                .create_batch(metrics, &self.config.agent.id, &self.hostname);

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

    pub async fn run(&mut self) -> Result<(), AgentError> {
        println!("Starting Operion Sentinel Agent...");
        println!("Agent ID: {}", self.config.agent.id);
        println!("API Endpoint: {}", self.api_client.endpoint());
        println!(
            "Collection interval: {} seconds",
            self.config.collection.interval_seconds
        );
        println!(
            "Flush interval: {} seconds",
            self.config.get_flush_interval_seconds()
        );

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
    #[error("API error: {0}")]
    Api(#[from] ApiError),
    #[error("Metric collection error: {0}")]
    MetricCollection(String),
}
