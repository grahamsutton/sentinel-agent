use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Cloud provider instance metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceMetadata {
    pub instance_id: Option<String>,
    pub cloud_provider: Option<CloudProvider>,
    pub region: Option<String>,
    pub instance_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CloudProvider {
    AWS,
    Azure,
    GCP,
    DigitalOcean,
    Unknown,
}

impl InstanceMetadata {
    /// Detect cloud instance metadata from the environment
    pub async fn detect() -> Self {
        // Try AWS first (most common)
        if let Some(aws_meta) = Self::fetch_aws_metadata().await {
            return aws_meta;
        }

        // Try Azure
        if let Some(azure_meta) = Self::fetch_azure_metadata().await {
            return azure_meta;
        }

        // Try GCP
        if let Some(gcp_meta) = Self::fetch_gcp_metadata().await {
            return gcp_meta;
        }

        // Try DigitalOcean
        if let Some(do_meta) = Self::fetch_digitalocean_metadata().await {
            return do_meta;
        }

        // Not in a recognized cloud environment
        Self {
            instance_id: None,
            cloud_provider: None,
            region: None,
            instance_type: None,
        }
    }

    /// Fetch AWS EC2 instance metadata
    async fn fetch_aws_metadata() -> Option<Self> {
        // AWS IMDSv2 (Instance Metadata Service v2) - more secure
        // First get the token
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(500)) // Fast timeout for non-AWS environments
            .build()
            .ok()?;

        // Try to get IMDSv2 token
        let token_response = client
            .put("http://169.254.169.254/latest/api/token")
            .header("X-aws-ec2-metadata-token-ttl-seconds", "21600")
            .send()
            .await
            .ok()?;

        if !token_response.status().is_success() {
            // Try IMDSv1 fallback
            return Self::fetch_aws_metadata_v1().await;
        }

        let token = token_response.text().await.ok()?;

        // Fetch instance ID
        let instance_id = client
            .get("http://169.254.169.254/latest/meta-data/instance-id")
            .header("X-aws-ec2-metadata-token", &token)
            .send()
            .await
            .ok()?
            .text()
            .await
            .ok()?;

        // Fetch instance type
        let instance_type = client
            .get("http://169.254.169.254/latest/meta-data/instance-type")
            .header("X-aws-ec2-metadata-token", &token)
            .send()
            .await
            .ok()?
            .text()
            .await
            .ok();

        // Fetch region
        let region = client
            .get("http://169.254.169.254/latest/meta-data/placement/region")
            .header("X-aws-ec2-metadata-token", &token)
            .send()
            .await
            .ok()?
            .text()
            .await
            .ok();

        Some(Self {
            instance_id: Some(instance_id),
            cloud_provider: Some(CloudProvider::AWS),
            region,
            instance_type,
        })
    }

    /// Fetch AWS metadata using IMDSv1 (fallback)
    async fn fetch_aws_metadata_v1() -> Option<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .ok()?;

        let instance_id = client
            .get("http://169.254.169.254/latest/meta-data/instance-id")
            .send()
            .await
            .ok()?
            .text()
            .await
            .ok()?;

        Some(Self {
            instance_id: Some(instance_id),
            cloud_provider: Some(CloudProvider::AWS),
            region: None,
            instance_type: None,
        })
    }

    /// Fetch Azure instance metadata
    async fn fetch_azure_metadata() -> Option<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .ok()?;

        #[derive(Deserialize)]
        struct AzureMetadata {
            compute: AzureCompute,
        }

        #[derive(Deserialize)]
        struct AzureCompute {
            #[serde(rename = "vmId")]
            vm_id: String,
            location: Option<String>,
            #[serde(rename = "vmSize")]
            vm_size: Option<String>,
        }

        let response = client
            .get("http://169.254.169.254/metadata/instance")
            .header("Metadata", "true")
            .query(&[("api-version", "2021-02-01")])
            .send()
            .await
            .ok()?;

        let metadata: AzureMetadata = response.json().await.ok()?;

        Some(Self {
            instance_id: Some(metadata.compute.vm_id),
            cloud_provider: Some(CloudProvider::Azure),
            region: metadata.compute.location,
            instance_type: metadata.compute.vm_size,
        })
    }

    /// Fetch GCP instance metadata
    async fn fetch_gcp_metadata() -> Option<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .ok()?;

        let instance_id = client
            .get("http://metadata.google.internal/computeMetadata/v1/instance/id")
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .ok()?
            .text()
            .await
            .ok()?;

        let zone = client
            .get("http://metadata.google.internal/computeMetadata/v1/instance/zone")
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .ok()?
            .text()
            .await
            .ok();

        // Extract region from zone (e.g., "projects/123/zones/us-central1-a" -> "us-central1")
        let region = zone.as_ref().and_then(|z| {
            z.split('/').last()?.rsplit_once('-').map(|(r, _)| r.to_string())
        });

        Some(Self {
            instance_id: Some(instance_id),
            cloud_provider: Some(CloudProvider::GCP),
            region,
            instance_type: None,
        })
    }

    /// Fetch DigitalOcean droplet metadata
    async fn fetch_digitalocean_metadata() -> Option<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .ok()?;

        #[derive(Deserialize)]
        struct DropletMetadata {
            droplet_id: u64,
            region: Option<String>,
        }

        let response = client
            .get("http://169.254.169.254/metadata/v1.json")
            .send()
            .await
            .ok()?;

        let metadata: DropletMetadata = response.json().await.ok()?;

        Some(Self {
            instance_id: Some(metadata.droplet_id.to_string()),
            cloud_provider: Some(CloudProvider::DigitalOcean),
            region: metadata.region,
            instance_type: None,
        })
    }
}

/// Session information for tracking agent runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// System boot time (Unix timestamp)
    pub boot_time: u64,
    /// When this agent process started (Unix timestamp)
    pub agent_start_time: u64,
    /// Current system uptime in seconds
    pub uptime_seconds: u64,
}

impl SessionInfo {
    /// Generate current session information
    pub fn generate() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let agent_start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            boot_time: sysinfo::System::boot_time(),
            agent_start_time,
            uptime_seconds: sysinfo::System::uptime(),
        }
    }

    /// Check if this session is consistent with a previous one
    pub fn is_consistent_with(&self, previous: &SessionInfo, elapsed_seconds: u64) -> bool {
        // Boot time should never change for the same instance
        if self.boot_time != previous.boot_time {
            return false;
        }

        // Agent start time should remain the same for the same agent process
        if self.agent_start_time != previous.agent_start_time {
            // Different agent process, but same boot = agent restart (acceptable)
            return true;
        }

        // Uptime should increase by approximately elapsed time
        // Allow 10% tolerance for clock drift and measurement delays
        let expected_uptime = previous.uptime_seconds + elapsed_seconds;
        let uptime_diff = (self.uptime_seconds as i64 - expected_uptime as i64).abs();
        let tolerance = (elapsed_seconds as f64 * 0.1) as i64 + 5; // 10% or minimum 5 seconds

        uptime_diff <= tolerance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_info_generation() {
        let session = SessionInfo::generate();

        assert!(session.boot_time > 0);
        assert!(session.agent_start_time > 0);
        assert!(session.uptime_seconds > 0);
        assert!(session.agent_start_time >= session.boot_time);
    }

    #[test]
    fn test_session_consistency() {
        let session1 = SessionInfo {
            boot_time: 1700000000,
            agent_start_time: 1700001000,
            uptime_seconds: 1000,
        };

        // Same boot time, agent time, uptime increased = consistent
        let session2 = SessionInfo {
            boot_time: 1700000000,
            agent_start_time: 1700001000,
            uptime_seconds: 1060,
        };
        assert!(session2.is_consistent_with(&session1, 60));

        // Different boot time = inconsistent (different machine or reboot)
        let session3 = SessionInfo {
            boot_time: 1700002000,
            agent_start_time: 1700003000,
            uptime_seconds: 100,
        };
        assert!(!session3.is_consistent_with(&session1, 60));

        // Same boot, different agent start = agent restart (acceptable)
        let session4 = SessionInfo {
            boot_time: 1700000000,
            agent_start_time: 1700002000, // Agent restarted
            uptime_seconds: 2000,
        };
        assert!(session4.is_consistent_with(&session1, 1000));
    }

    #[tokio::test]
    async fn test_instance_metadata_detection() {
        // This will return empty metadata in dev environment
        // but will detect actual cloud metadata when running in cloud
        let metadata = InstanceMetadata::detect().await;

        // In development, we expect no cloud provider
        if metadata.cloud_provider.is_none() {
            assert!(metadata.instance_id.is_none());
            assert!(metadata.region.is_none());
        }
    }
}