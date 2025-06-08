use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub agent: AgentConfig,
    pub api: ApiConfig,
    pub collection: CollectionConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub id: String,
    pub hostname: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApiConfig {
    pub endpoint: String,
    pub timeout_seconds: Option<u64>,
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CollectionConfig {
    pub interval_seconds: u64,
    pub batch_size: Option<usize>,
    pub flush_interval_seconds: Option<u64>,
    pub disk: DiskConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DiskConfig {
    pub enabled: bool,
    pub include_mount_points: Option<Vec<String>>,
    pub exclude_mount_points: Option<Vec<String>>,
}

impl Config {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let contents =
            std::fs::read_to_string(path).map_err(|e| ConfigError::FileRead(e.to_string()))?;

        let config: Config =
            serde_yaml::from_str(&contents).map_err(|e| ConfigError::Parse(e.to_string()))?;

        config.validate()?;
        Ok(config)
    }

    #[allow(dead_code)]
    pub fn load_from_str(contents: &str) -> Result<Self, ConfigError> {
        let config: Config =
            serde_yaml::from_str(contents).map_err(|e| ConfigError::Parse(e.to_string()))?;

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.agent.id.is_empty() {
            return Err(ConfigError::Validation(
                "Agent ID cannot be empty".to_string(),
            ));
        }

        if self.api.endpoint.is_empty() {
            return Err(ConfigError::Validation(
                "API endpoint cannot be empty".to_string(),
            ));
        }

        if self.collection.interval_seconds == 0 {
            return Err(ConfigError::Validation(
                "Collection interval must be greater than 0".to_string(),
            ));
        }

        // Validate API key if present
        if let Some(api_key) = &self.api.api_key {
            if api_key.trim().is_empty() {
                return Err(ConfigError::Validation(
                    "API key cannot be empty".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub fn get_hostname(&self) -> String {
        self.agent
            .hostname
            .clone()
            .unwrap_or_else(|| gethostname::gethostname().to_string_lossy().to_string())
    }

    pub fn get_api_timeout_seconds(&self) -> u64 {
        self.api.timeout_seconds.unwrap_or(30)
    }

    pub fn get_batch_size(&self) -> usize {
        self.collection.batch_size.unwrap_or(100)
    }

    pub fn get_flush_interval_seconds(&self) -> u64 {
        self.collection.flush_interval_seconds.unwrap_or(10)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    FileRead(String),
    #[error("Failed to parse config file: {0}")]
    Parse(String),
    #[error("Config validation error: {0}")]
    Validation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_valid_config_yaml() -> String {
        r#"
agent:
  id: "test-agent"
  hostname: "test-host"
api:
  endpoint: "https://api.example.com"
  timeout_seconds: 30
collection:
  interval_seconds: 60
  batch_size: 100
  flush_interval_seconds: 10
  disk:
    enabled: true
"#.to_string()
    }

    #[test]
    fn test_load_valid_config_from_str() {
        let yaml = create_valid_config_yaml();
        let config = Config::load_from_str(&yaml).unwrap();
        assert_eq!(config.agent.id, "test-agent");
        assert_eq!(config.agent.hostname, Some("test-host".to_string()));
        assert_eq!(config.api.endpoint, "https://api.example.com");
    }

    #[test]
    fn test_config_validation_empty_agent_id() {
        let yaml = r#"
agent:
  id: ""
api:
  endpoint: "https://api.example.com"
collection:
  interval_seconds: 60
  disk:
    enabled: true
"#;
        let result = Config::load_from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_defaults() {
        let yaml = create_valid_config_yaml();
        let config = Config::load_from_str(&yaml).unwrap();
        assert_eq!(config.get_api_timeout_seconds(), 30);
        assert_eq!(config.get_batch_size(), 100);
        assert_eq!(config.get_flush_interval_seconds(), 10);
    }
}
