//! Lago configuration management.

use serde::{Deserialize, Serialize};

/// Configuration for Lago deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LagoConfig {
    /// The API URL for Lago (default: `http://localhost:3000`)
    pub api_url: String,

    /// The front-end URL for Lago (default: `http://localhost:80`)
    pub front_url: String,

    /// `PostgreSQL` database URL
    pub database_url: String,

    /// Redis URL for caching and jobs
    pub redis_url: String,

    /// Secret key base for Rails encryption
    pub secret_key_base: String,

    /// RSA private key for JWT signing (optional)
    pub rsa_private_key: Option<String>,

    /// Lago API key for authentication
    pub api_key: Option<String>,

    /// Enable or disable signup (default: true)
    pub disable_signup: bool,
}

impl Default for LagoConfig {
    fn default() -> Self {
        Self {
            api_url: "http://localhost:3000".to_string(),
            front_url: "http://localhost:80".to_string(),
            database_url: "postgresql://lago:lago@localhost:5432/lago".to_string(),
            redis_url: "redis://localhost:6379".to_string(),
            secret_key_base: generate_secret_key(),
            rsa_private_key: None,
            api_key: None,
            disable_signup: false,
        }
    }
}

impl LagoConfig {
    /// Create a new Lago configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the API URL.
    #[must_use]
    pub fn with_api_url(mut self, url: impl Into<String>) -> Self {
        self.api_url = url.into();
        self
    }

    /// Set the front-end URL.
    #[must_use]
    pub fn with_front_url(mut self, url: impl Into<String>) -> Self {
        self.front_url = url.into();
        self
    }

    /// Set the database URL.
    #[must_use]
    pub fn with_database_url(mut self, url: impl Into<String>) -> Self {
        self.database_url = url.into();
        self
    }

    /// Set the Redis URL.
    #[must_use]
    pub fn with_redis_url(mut self, url: impl Into<String>) -> Self {
        self.redis_url = url.into();
        self
    }

    /// Set the API key.
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Convert configuration to environment variables for Docker Compose.
    #[must_use]
    pub fn to_env_vars(&self) -> Vec<(String, String)> {
        let mut vars = vec![
            ("LAGO_API_URL".to_string(), self.api_url.clone()),
            ("LAGO_FRONT_URL".to_string(), self.front_url.clone()),
            ("DATABASE_URL".to_string(), self.database_url.clone()),
            ("REDIS_URL".to_string(), self.redis_url.clone()),
            ("SECRET_KEY_BASE".to_string(), self.secret_key_base.clone()),
            (
                "LAGO_DISABLE_SIGNUP".to_string(),
                self.disable_signup.to_string(),
            ),
        ];

        if let Some(ref key) = self.rsa_private_key {
            vars.push(("LAGO_RSA_PRIVATE_KEY".to_string(), key.clone()));
        }

        if let Some(ref key) = self.api_key {
            vars.push(("LAGO_API_KEY".to_string(), key.clone()));
        }

        vars
    }
}

/// Generate a random secret key for Rails.
fn generate_secret_key() -> String {
    use std::fmt::Write;
    let mut key = String::with_capacity(128);
    for i in 0..64 {
        // Simple hex encoding of pseudo-random bytes based on time and iteration
        // In production, you'd want a proper random generator
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        #[allow(clippy::cast_possible_truncation)]
        let byte = ((nanos.wrapping_add(i * 37)) % 256) as u8;
        write!(&mut key, "{byte:02x}").ok();
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LagoConfig::default();
        assert_eq!(config.api_url, "http://localhost:3000");
        assert_eq!(config.front_url, "http://localhost:80");
        assert!(!config.disable_signup);
    }

    #[test]
    fn test_builder_pattern() {
        let config = LagoConfig::new()
            .with_api_url("http://api.example.com")
            .with_api_key("test-key");

        assert_eq!(config.api_url, "http://api.example.com");
        assert_eq!(config.api_key, Some("test-key".to_string()));
    }

    #[test]
    fn test_env_vars() {
        let config = LagoConfig::new().with_api_key("my-key");
        let vars = config.to_env_vars();

        assert!(vars.iter().any(|(k, v)| k == "LAGO_API_URL" && v == "http://localhost:3000"));
        assert!(vars.iter().any(|(k, v)| k == "LAGO_API_KEY" && v == "my-key"));
    }
}
