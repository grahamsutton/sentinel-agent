use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs;

use crate::config::Config;

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredToken {
    access_token: String,
    token_type: String,
    expires_at: u64,
    scope: Option<String>,
}

pub struct OAuthManager {
    client: Client,
    client_id: String,
    client_secret: String,
    token_endpoint: String,
    scope: Option<String>,
    token_cache_path: PathBuf,
}

impl OAuthManager {
    pub fn new(config: &Config) -> Result<Self, OAuthError> {
        let oauth_config = config.api.oauth.as_ref()
            .ok_or(OAuthError::Configuration("OAuth not configured".to_string()))?;

        let token_endpoint = config.get_oauth_token_endpoint()
            .ok_or(OAuthError::Configuration("Token endpoint not configured".to_string()))?;

        let scope = config.get_oauth_scope();

        // Create secure token cache directory
        let cache_dir = dirs::cache_dir()
            .ok_or(OAuthError::Storage("Cannot determine cache directory".to_string()))?
            .join("operion")
            .join("sentinel-agent");

        let token_cache_path = cache_dir.join("oauth_token.json");

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| OAuthError::Http(e.to_string()))?;

        Ok(Self {
            client,
            client_id: oauth_config.client_id.clone(),
            client_secret: oauth_config.client_secret.clone(),
            token_endpoint,
            scope,
            token_cache_path,
        })
    }

    pub async fn get_access_token(&mut self) -> Result<String, OAuthError> {
        // Try to load cached token first
        if let Ok(token) = self.load_cached_token().await {
            if !self.is_token_expired(&token) {
                return Ok(token.access_token);
            }
        }

        // Request new token
        let new_token = self.request_new_token().await?;
        
        // Cache the new token
        self.cache_token(&new_token).await?;
        
        Ok(new_token.access_token)
    }

    async fn request_new_token(&self) -> Result<TokenResponse, OAuthError> {
        let mut params = vec![
            ("grant_type", "client_credentials"),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
        ];

        if let Some(scope) = &self.scope {
            params.push(("scope", scope));
        }

        let response = self
            .client
            .post(&self.token_endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("User-Agent", "OperionSentinelAgent/1.0")
            .form(&params)
            .send()
            .await
            .map_err(|e| OAuthError::Http(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            
            return Err(OAuthError::TokenRequest {
                status: status.as_u16(),
                body,
            });
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| OAuthError::Parse(e.to_string()))?;

        Ok(token_response)
    }

    async fn load_cached_token(&self) -> Result<StoredToken, OAuthError> {
        let content = fs::read_to_string(&self.token_cache_path)
            .await
            .map_err(|_| OAuthError::Storage("No cached token found".to_string()))?;

        let token: StoredToken = serde_json::from_str(&content)
            .map_err(|e| OAuthError::Parse(e.to_string()))?;

        Ok(token)
    }

    async fn cache_token(&self, token: &TokenResponse) -> Result<(), OAuthError> {
        // Ensure cache directory exists
        if let Some(parent) = self.token_cache_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| OAuthError::Storage(e.to_string()))?;
        }

        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() + token.expires_in - 60; // Subtract 60 seconds for safety buffer

        let stored_token = StoredToken {
            access_token: token.access_token.clone(),
            token_type: token.token_type.clone(),
            expires_at,
            scope: token.scope.clone(),
        };

        let content = serde_json::to_string_pretty(&stored_token)
            .map_err(|e| OAuthError::Parse(e.to_string()))?;

        fs::write(&self.token_cache_path, content)
            .await
            .map_err(|e| OAuthError::Storage(e.to_string()))?;

        // Set restrictive permissions on token cache file (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&self.token_cache_path)
                .await
                .map_err(|e| OAuthError::Storage(e.to_string()))?
                .permissions();
            perms.set_mode(0o600); // Read/write for owner only
            fs::set_permissions(&self.token_cache_path, perms)
                .await
                .map_err(|e| OAuthError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    fn is_token_expired(&self, token: &StoredToken) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        now >= token.expires_at
    }

    pub async fn clear_cached_token(&self) -> Result<(), OAuthError> {
        if self.token_cache_path.exists() {
            fs::remove_file(&self.token_cache_path)
                .await
                .map_err(|e| OAuthError::Storage(e.to_string()))?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("OAuth configuration error: {0}")]
    Configuration(String),
    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("Token request failed with status {status}: {body}")]
    TokenRequest { status: u16, body: String },
    #[error("Failed to parse response: {0}")]
    Parse(String),
    #[error("Token storage error: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn create_test_config_with_oauth(token_endpoint: &str) -> Config {
        Config::load_from_str(&format!(r#"
agent:
  id: "test-agent"
api:
  endpoint: "https://api.operion.co"
  timeout_seconds: 5
  oauth:
    client_id: "test-client-id"
    client_secret: "test-client-secret"
    token_endpoint: "{}"
    scope: "server:register server:metrics"
collection:
  interval_seconds: 60
  disk:
    enabled: true
"#, token_endpoint)).unwrap()
    }

    #[tokio::test]
    async fn test_oauth_manager_creation() {
        let mock_server = MockServer::start().await;
        let config = create_test_config_with_oauth(&format!("{}/oauth/token", mock_server.uri())).await;
        
        let manager = OAuthManager::new(&config);
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_successful_token_request() {
        let mock_server = MockServer::start().await;
        
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&serde_json::json!({
                "access_token": "test-access-token",
                "token_type": "Bearer",
                "expires_in": 3600,
                "scope": "server:register server:metrics"
            })))
            .mount(&mock_server)
            .await;

        let config = create_test_config_with_oauth(&format!("{}/oauth/token", mock_server.uri())).await;
        let mut manager = OAuthManager::new(&config).unwrap();

        let token = manager.get_access_token().await;
        assert!(token.is_ok());
        assert_eq!(token.unwrap(), "test-access-token");
    }

    #[tokio::test]
    async fn test_token_caching() {
        let mock_server = MockServer::start().await;
        
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&serde_json::json!({
                "access_token": "test-access-token",
                "token_type": "Bearer",
                "expires_in": 3600,
                "scope": "server:register server:metrics"
            })))
            .expect(1) // Should only be called once due to caching
            .mount(&mock_server)
            .await;

        let config = create_test_config_with_oauth(&format!("{}/oauth/token", mock_server.uri())).await;
        let mut manager = OAuthManager::new(&config).unwrap();

        // First request - should hit the server
        let token1 = manager.get_access_token().await.unwrap();
        
        // Second request - should use cached token
        let token2 = manager.get_access_token().await.unwrap();
        
        assert_eq!(token1, token2);
    }
}