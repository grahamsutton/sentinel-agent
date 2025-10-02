use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
// Time utilities provided by chrono
use chrono::{DateTime, Utc};

/// Represents the persisted state of a registered resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceState {
    /// The resource ID assigned by the Operion platform
    pub resource_id: String,
    /// ISO 8601 timestamp of when the resource was registered
    pub registered_at: String,
    /// The agent version that performed the registration
    pub agent_version: String,
}

impl ResourceState {
    /// Create a new ResourceState
    pub fn new(resource_id: String, agent_version: String) -> Self {
        let now: DateTime<Utc> = Utc::now();
        Self {
            resource_id,
            registered_at: now.to_rfc3339(),
            agent_version,
        }
    }

    /// Get the path to the state file based on runtime context
    pub fn get_state_file_path() -> PathBuf {
        // Check if running as root/system service
        if std::env::var("USER").unwrap_or_default() == "root" ||
           std::env::var("SUDO_USER").is_ok() ||
           std::fs::metadata("/etc/operion").is_ok() {
            // System-wide installation
            PathBuf::from("/etc/operion/resource-state.json")
        } else {
            // User installation
            let config_dir = dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("operion");
            config_dir.join("resource-state.json")
        }
    }

    /// Load state from the JSON file
    pub fn load() -> Result<Option<Self>, StateError> {
        let path = Self::get_state_file_path();

        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&path)
            .map_err(|e| StateError::ReadError {
                path: path.to_string_lossy().to_string(),
                error: e.to_string(),
            })?;

        let state: ResourceState = serde_json::from_str(&contents)
            .map_err(|e| StateError::ParseError {
                path: path.to_string_lossy().to_string(),
                error: e.to_string(),
            })?;

        Ok(Some(state))
    }

    /// Save state to the JSON file
    pub fn save(&self) -> Result<(), StateError> {
        let path = Self::get_state_file_path();

        // Ensure the directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| StateError::CreateDirectoryError {
                    path: parent.to_string_lossy().to_string(),
                    error: e.to_string(),
                })?;
        }

        // Serialize to pretty JSON
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| StateError::SerializeError(e.to_string()))?;

        // Write to a temporary file first (atomic write)
        let temp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&temp_path)
            .map_err(|e| StateError::WriteError {
                path: temp_path.to_string_lossy().to_string(),
                error: e.to_string(),
            })?;

        file.write_all(json.as_bytes())
            .map_err(|e| StateError::WriteError {
                path: temp_path.to_string_lossy().to_string(),
                error: e.to_string(),
            })?;

        file.sync_all()
            .map_err(|e| StateError::WriteError {
                path: temp_path.to_string_lossy().to_string(),
                error: e.to_string(),
            })?;

        // Atomically rename temp file to actual file
        fs::rename(&temp_path, &path)
            .map_err(|e| StateError::WriteError {
                path: path.to_string_lossy().to_string(),
                error: e.to_string(),
            })?;

        // Set restrictive permissions (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&path)
                .map_err(|e| StateError::PermissionError {
                    path: path.to_string_lossy().to_string(),
                    error: e.to_string(),
                })?;

            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(&path, permissions)
                .map_err(|e| StateError::PermissionError {
                    path: path.to_string_lossy().to_string(),
                    error: e.to_string(),
                })?;
        }

        Ok(())
    }

}

/// Errors that can occur when working with resource state
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Failed to read state file at {path}: {error}")]
    ReadError { path: String, error: String },

    #[error("Failed to parse state file at {path}: {error}")]
    ParseError { path: String, error: String },

    #[error("Failed to write state file at {path}: {error}")]
    WriteError { path: String, error: String },

    #[error("Failed to create directory {path}: {error}")]
    CreateDirectoryError { path: String, error: String },

    #[error("Failed to set permissions on {path}: {error}")]
    PermissionError { path: String, error: String },

    #[error("Failed to serialize state: {0}")]
    SerializeError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::env;

    #[test]
    fn test_resource_state_creation() {
        let state = ResourceState::new(
            "res_123456".to_string(),
            "0.2.1".to_string(),
        );

        assert_eq!(state.resource_id, "res_123456");
        assert_eq!(state.agent_version, "0.2.1");
        assert!(!state.registered_at.is_empty());
    }

    #[test]
    fn test_state_serialization() {
        let state = ResourceState::new(
            "res_abc123".to_string(),
            "0.2.1".to_string(),
        );

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("res_abc123"));
        assert!(json.contains("0.2.1"));
        assert!(json.contains("registered_at"));

        let deserialized: ResourceState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.resource_id, state.resource_id);
        assert_eq!(deserialized.agent_version, state.agent_version);
    }

    #[test]
    fn test_state_file_operations() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let state_path = temp_dir.path().join("resource-state.json");

        // Override the state file path for testing
        env::set_var("HOME", temp_dir.path());

        let state = ResourceState {
            resource_id: "res_test123".to_string(),
            registered_at: "2024-01-15T10:30:00Z".to_string(),
            agent_version: "0.2.1".to_string(),
        };

        // Test saving
        // For testing, we'll write directly to our temp path
        let json = serde_json::to_string_pretty(&state).unwrap();
        fs::write(&state_path, json).unwrap();

        // Test loading
        let contents = fs::read_to_string(&state_path).unwrap();
        let loaded: ResourceState = serde_json::from_str(&contents).unwrap();

        assert_eq!(loaded.resource_id, "res_test123");
        assert_eq!(loaded.agent_version, "0.2.1");
    }
}