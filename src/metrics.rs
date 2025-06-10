use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::Disks;

use crate::config::{Config, DiskConfig};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DiskMetric {
    pub timestamp: u64,
    pub device: String,
    pub mount_point: String,
    pub total_space_bytes: u64,
    pub used_space_bytes: u64,
    pub available_space_bytes: u64,
    pub usage_percentage: f64,
}

#[derive(Serialize, Debug)]
pub struct MetricBatch {
    pub server_id: String,
    pub hostname: String,
    pub timestamp: u64,
    pub metrics: Vec<DiskMetric>,
}

pub trait MetricCollector {
    type Metric;
    type Error;

    fn collect(&self) -> Result<Vec<Self::Metric>, Self::Error>;
    fn is_enabled(&self) -> bool;
}

pub struct DiskCollector {
    config: DiskConfig,
}

impl DiskCollector {
    pub fn new(config: DiskConfig) -> Self {
        Self { config }
    }

    fn should_include_mount_point(&self, mount_point: &str) -> bool {
        // Check include list first
        if let Some(ref include_list) = self.config.include_mount_points {
            if !include_list
                .iter()
                .any(|pattern| mount_point.contains(pattern))
            {
                return false;
            }
        }

        // Check exclude list
        if let Some(ref exclude_list) = self.config.exclude_mount_points {
            if exclude_list
                .iter()
                .any(|pattern| mount_point.contains(pattern))
            {
                return false;
            }
        }

        true
    }

    fn create_disk_metric(&self, disk: &sysinfo::Disk, timestamp: u64) -> DiskMetric {
        let total_space = disk.total_space();
        let available_space = disk.available_space();
        let used_space = total_space - available_space;
        let usage_percentage = if total_space > 0 {
            used_space as f64 / total_space as f64
        } else {
            0.0
        };

        DiskMetric {
            timestamp,
            device: disk.name().to_string_lossy().to_string(),
            mount_point: disk.mount_point().to_string_lossy().to_string(),
            total_space_bytes: total_space,
            used_space_bytes: used_space,
            available_space_bytes: available_space,
            usage_percentage,
        }
    }
}

impl MetricCollector for DiskCollector {
    type Metric = DiskMetric;
    type Error = MetricError;

    fn collect(&self) -> Result<Vec<Self::Metric>, Self::Error> {
        if !self.is_enabled() {
            return Ok(Vec::new());
        }

        let disks = Disks::new_with_refreshed_list();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| MetricError::TimestampError)?
            .as_secs();

        let metrics = disks
            .iter()
            .filter_map(|disk| {
                let mount_point = disk.mount_point().to_string_lossy();
                if self.should_include_mount_point(&mount_point) {
                    Some(self.create_disk_metric(disk, timestamp))
                } else {
                    None
                }
            })
            .collect();

        Ok(metrics)
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

pub struct MetricService {
    disk_collector: DiskCollector,
}

impl MetricService {
    pub fn new(config: &Config) -> Self {
        Self {
            disk_collector: DiskCollector::new(config.collection.disk.clone()),
        }
    }

    pub fn collect_all_metrics(&self) -> Result<Vec<DiskMetric>, MetricError> {
        let mut all_metrics = Vec::new();

        // Collect disk metrics
        let disk_metrics = self.disk_collector.collect()?;
        all_metrics.extend(disk_metrics);

        Ok(all_metrics)
    }

    pub fn create_batch(
        &self,
        metrics: Vec<DiskMetric>,
        server_id: &str,
        hostname: &str,
    ) -> MetricBatch {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        MetricBatch {
            server_id: server_id.to_string(),
            hostname: hostname.to_string(),
            timestamp,
            metrics,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MetricError {
    #[error("Failed to get system timestamp")]
    TimestampError,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_disk_config() -> DiskConfig {
        DiskConfig {
            enabled: true,
            include_mount_points: None,
            exclude_mount_points: None,
        }
    }

    #[test]
    fn test_disk_collector_enabled() {
        let config = create_disk_config();
        let collector = DiskCollector::new(config);
        assert!(collector.is_enabled());
    }

    #[test]
    fn test_disk_collector_disabled() {
        let mut config = create_disk_config();
        config.enabled = false;
        let collector = DiskCollector::new(config);
        assert!(!collector.is_enabled());
    }

    #[test]
    fn test_mount_point_filtering_include() {
        let mut config = create_disk_config();
        config.include_mount_points = Some(vec!["/home".to_string()]);
        let collector = DiskCollector::new(config);

        assert!(!collector.should_include_mount_point("/"));
        assert!(collector.should_include_mount_point("/home"));
        assert!(!collector.should_include_mount_point("/dev/shm"));
    }

    #[test]
    fn test_mount_point_filtering_exclude() {
        let mut config = create_disk_config();
        config.exclude_mount_points = Some(vec!["/dev".to_string(), "/proc".to_string()]);
        let collector = DiskCollector::new(config);

        assert!(collector.should_include_mount_point("/"));
        assert!(collector.should_include_mount_point("/home"));
        assert!(!collector.should_include_mount_point("/dev/shm"));
        assert!(!collector.should_include_mount_point("/proc/fs"));
    }

    #[test]
    fn test_metric_batch_creation() {
        let metric = DiskMetric {
            timestamp: 1234567890,
            device: "/dev/sda1".to_string(),
            mount_point: "/".to_string(),
            total_space_bytes: 1000000,
            used_space_bytes: 500000,
            available_space_bytes: 500000,
            usage_percentage: 0.5,
        };

        let config = Config::load_from_str(r#"
agent:
  id: "test-agent"
api:
  endpoint: "https://api.example.com"
collection:
  interval_seconds: 60
  disk:
    enabled: true
"#).unwrap();

        let service = MetricService::new(&config);
        let batch = service.create_batch(vec![metric], "test-id", "test-host");

        assert_eq!(batch.server_id, "test-id");
        assert_eq!(batch.hostname, "test-host");
        assert_eq!(batch.metrics.len(), 1);
    }

    #[test]
    fn test_collect_disabled() {
        let mut config = create_disk_config();
        config.enabled = false;
        let collector = DiskCollector::new(config);

        let result = collector.collect().unwrap();
        assert!(result.is_empty());
    }
}
