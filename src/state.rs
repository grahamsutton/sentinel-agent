use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
// Time utilities provided by chrono
use chrono::{DateTime, Utc};
use crate::metadata::{InstanceMetadata, SessionInfo};

/// Represents the persisted state of a registered resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceState {
    /// The resource ID assigned by the Operion platform
    pub resource_id: String,
    /// ISO 8601 timestamp of when the resource was registered
    pub registered_at: String,
    /// The agent version that performed the registration
    pub agent_version: String,
    /// Cloud instance metadata from registration
    pub instance_metadata: InstanceMetadata,
    /// Session info from when the agent started
    pub session: SessionInfo,
}

impl ResourceState {
    /// Create a new ResourceState
    pub fn new(
        resource_id: String,
        agent_version: String,
        instance_metadata: InstanceMetadata,
        session: SessionInfo,
    ) -> Self {
        let now: DateTime<Utc> = Utc::now();
        Self {
            resource_id,
            registered_at: now.to_rfc3339(),
            agent_version,
            instance_metadata,
            session,
        }
    }

    /// Get the path to the state file based on runtime context
    ///
    /// Priority order:
    /// 1. /var/lib/operion (preferred for system services - writable by service user)
    /// 2. /etc/operion (legacy system-wide location)
    /// 3. ~/.config/operion (user installation fallback)
    pub fn get_state_file_path() -> PathBuf {
        // Try /var/lib/operion first (best practice for system service state)
        let var_lib_path = PathBuf::from("/var/lib/operion/resource-state.json");
        if let Some(parent) = var_lib_path.parent() {
            if parent.exists() || Self::can_create_directory(parent) {
                return var_lib_path;
            }
        }

        // Try /etc/operion (legacy location)
        let etc_path = PathBuf::from("/etc/operion/resource-state.json");
        if let Some(parent) = etc_path.parent() {
            if parent.exists() || Self::can_create_directory(parent) {
                return etc_path;
            }
        }

        // Fallback to user config directory
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("operion");
        config_dir.join("resource-state.json")
    }

    /// Check if we can create a directory (by attempting to create it)
    fn can_create_directory(path: &std::path::Path) -> bool {
        // If parent doesn't exist, we can't create it
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                return false;
            }
        }

        // Try to create the directory
        fs::create_dir_all(path).is_ok()
    }

    /// Load state from the JSON file
    ///
    /// Searches for the state file in multiple locations in priority order
    pub fn load() -> Result<Option<Self>, StateError> {
        // Try loading from different locations in priority order
        let paths_to_try = vec![
            PathBuf::from("/var/lib/operion/resource-state.json"),
            PathBuf::from("/etc/operion/resource-state.json"),
            {
                let config_dir = dirs::config_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("operion");
                config_dir.join("resource-state.json")
            },
        ];

        for path in paths_to_try {
            if !path.exists() {
                continue;
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

            return Ok(Some(state));
        }

        // No state file found in any location
        Ok(None)
    }

    /// Save state to the JSON file
    pub fn save(&self) -> Result<(), StateError> {
        // Serialize to pretty JSON once
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| StateError::SerializeError(e.to_string()))?;

        // Try saving to different locations in priority order
        let paths_to_try = vec![
            PathBuf::from("/var/lib/operion/resource-state.json"),
            PathBuf::from("/etc/operion/resource-state.json"),
            {
                let config_dir = dirs::config_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("operion");
                config_dir.join("resource-state.json")
            },
        ];

        let mut last_error = None;

        for path in paths_to_try {
            match Self::try_save_to_path(&path, &json) {
                Ok(()) => return Ok(()),
                Err(e) => last_error = Some(e),
            }
        }

        // If all attempts failed, return the last error
        Err(last_error.unwrap_or_else(|| StateError::WriteError {
            path: "unknown".to_string(),
            error: "No writable location found".to_string(),
        }))
    }

    /// Attempt to save state to a specific path
    fn try_save_to_path(path: &PathBuf, json: &str) -> Result<(), StateError> {
        // Ensure the directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| StateError::CreateDirectoryError {
                    path: parent.to_string_lossy().to_string(),
                    error: e.to_string(),
                })?;
        }

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
        let instance_metadata = InstanceMetadata {
            instance_id: None,
            cloud_provider: None,
            region: None,
            instance_type: None,
        };
        let session = SessionInfo::generate();

        let state = ResourceState::new(
            "res_123456".to_string(),
            "0.2.1".to_string(),
            instance_metadata,
            session,
        );

        assert_eq!(state.resource_id, "res_123456");
        assert_eq!(state.agent_version, "0.2.1");
        assert!(!state.registered_at.is_empty());
    }

    #[test]
    fn test_state_serialization() {
        let instance_metadata = InstanceMetadata {
            instance_id: None,
            cloud_provider: None,
            region: None,
            instance_type: None,
        };
        let session = SessionInfo::generate();

        let state = ResourceState::new(
            "res_abc123".to_string(),
            "0.2.1".to_string(),
            instance_metadata,
            session,
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

        let instance_metadata = InstanceMetadata {
            instance_id: None,
            cloud_provider: None,
            region: None,
            instance_type: None,
        };
        let session = SessionInfo::generate();

        let state = ResourceState {
            resource_id: "res_test123".to_string(),
            registered_at: "2024-01-15T10:30:00Z".to_string(),
            agent_version: "0.2.1".to_string(),
            instance_metadata,
            session,
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