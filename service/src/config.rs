//! Service configuration.

use serde::Deserialize;
use std::path::Path;
use z_billing_core::PricingConfig;

/// Service configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Address to listen on (default: "0.0.0.0:8080").
    pub listen_addr: String,

    /// Path to `RocksDB` data directory (default: "/data/z-billing").
    pub data_dir: String,

    /// ZID JWT validation base URL (default: `<https://zid.zero.tech>`).
    pub auth_base_url: String,

    /// Expected JWT audience (default: "z-billing").
    pub auth_audience: String,

    /// Service API key for service-to-service auth.
    pub service_api_key: Option<String>,

    /// Lago API URL (optional).
    pub lago_api_url: Option<String>,

    /// Lago API key (optional).
    pub lago_api_key: Option<String>,

    /// Stripe API key (optional).
    pub stripe_api_key: Option<String>,

    /// Stripe webhook secret (optional).
    pub stripe_webhook_secret: Option<String>,

    /// Frontend URL for checkout redirects.
    pub frontend_url: String,

    /// CORS allowed origins.
    pub cors_origins: Vec<String>,

    /// Maximum request body size in bytes.
    pub max_body_bytes: usize,

    /// Request timeout in seconds.
    pub request_timeout_seconds: u64,

    /// Pricing configuration.
    pub pricing: PricingConfig,

    /// Lago organization ID (for reference).
    pub lago_organization_id: Option<String>,
}

/// Lago secrets file structure.
#[derive(Debug, Deserialize)]
struct LagoSecrets {
    api_url: String,
    api_key: String,
    #[serde(default)]
    organization_id: Option<String>,
}

/// Stripe secrets file structure.
#[derive(Debug, Deserialize)]
struct StripeSecrets {
    api_key: String,
    #[serde(default)]
    webhook_secret: Option<String>,
}

impl ServiceConfig {
    /// Load configuration from environment variables and secrets files.
    #[must_use]
    pub fn from_env() -> Self {
        // Try to load Lago secrets from file first, then fall back to env vars
        let (lago_api_url, lago_api_key, lago_organization_id) = load_lago_secrets();

        // Try to load Stripe secrets from file first, then fall back to env vars
        let (stripe_api_key, stripe_webhook_secret) = load_stripe_secrets();

        Self {
            listen_addr: std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            data_dir: std::env::var("DATA_DIR").unwrap_or_else(|_| "/data/z-billing".into()),
            auth_base_url: std::env::var("AUTH_BASE_URL")
                .unwrap_or_else(|_| "https://zid.zero.tech".into()),
            auth_audience: std::env::var("AUTH_AUDIENCE").unwrap_or_else(|_| "z-billing".into()),
            service_api_key: std::env::var("SERVICE_API_KEY").ok(),
            lago_api_url,
            lago_api_key,
            lago_organization_id,
            stripe_api_key,
            stripe_webhook_secret,
            frontend_url: std::env::var("FRONTEND_URL")
                .unwrap_or_else(|_| "http://localhost:3000".into()),
            cors_origins: std::env::var("CORS_ORIGINS")
                .unwrap_or_else(|_| "*".into())
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            max_body_bytes: std::env::var("MAX_BODY_BYTES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1024 * 1024), // 1MB
            request_timeout_seconds: std::env::var("REQUEST_TIMEOUT_SECONDS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            pricing: PricingConfig::default(),
        }
    }
}

/// Load Lago secrets from file or environment.
fn load_lago_secrets() -> (Option<String>, Option<String>, Option<String>) {
    // Try multiple paths for the secrets file
    let secret_paths = [
        ".secrets/lago.json",
        "z-billing/.secrets/lago.json",
        "z-billing/service/.secrets/lago.json",
        "../.secrets/lago.json",
    ];

    for path in &secret_paths {
        if let Ok(secrets) = load_secrets_file::<LagoSecrets>(path) {
            tracing::info!(path = %path, "Loaded Lago secrets from file");
            return (
                Some(secrets.api_url),
                Some(secrets.api_key),
                secrets.organization_id,
            );
        }
    }

    // Fall back to environment variables
    tracing::debug!("Lago secrets file not found, using environment variables");
    (
        std::env::var("LAGO_API_URL").ok(),
        std::env::var("LAGO_API_KEY").ok(),
        std::env::var("LAGO_ORGANIZATION_ID").ok(),
    )
}

/// Load Stripe secrets from file or environment.
fn load_stripe_secrets() -> (Option<String>, Option<String>) {
    let secret_paths = [
        ".secrets/stripe.json",
        "z-billing/.secrets/stripe.json",
        "z-billing/service/.secrets/stripe.json",
        "../.secrets/stripe.json",
    ];

    for path in &secret_paths {
        if let Ok(secrets) = load_secrets_file::<StripeSecrets>(path) {
            tracing::info!(path = %path, "Loaded Stripe secrets from file");
            return (Some(secrets.api_key), secrets.webhook_secret);
        }
    }

    // Fall back to environment variables
    tracing::debug!("Stripe secrets file not found, using environment variables");
    (
        std::env::var("STRIPE_API_KEY").ok(),
        std::env::var("STRIPE_WEBHOOK_SECRET").ok(),
    )
}

/// Load secrets from a JSON file.
fn load_secrets_file<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, std::io::Error> {
    let path = Path::new(path);
    if !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Secrets file not found",
        ));
    }
    let contents = std::fs::read_to_string(path)?;
    serde_json::from_str(&contents)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:8080".into(),
            data_dir: "/data/z-billing".into(),
            auth_base_url: "https://zid.zero.tech".into(),
            auth_audience: "z-billing".into(),
            service_api_key: None,
            lago_api_url: None,
            lago_api_key: None,
            lago_organization_id: None,
            stripe_api_key: None,
            stripe_webhook_secret: None,
            frontend_url: "http://localhost:3000".into(),
            cors_origins: vec!["*".into()],
            max_body_bytes: 1024 * 1024,
            request_timeout_seconds: 30,
            pricing: PricingConfig::default(),
        }
    }
}
