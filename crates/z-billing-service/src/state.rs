//! Application state.

use std::sync::Arc;

use z_billing_store::RocksStore;

use crate::config::ServiceConfig;
use crate::lago::LagoClient;
use crate::stripe::StripeClient;

/// Application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    /// The storage backend.
    pub store: Arc<RocksStore>,

    /// Service configuration.
    pub config: ServiceConfig,

    /// Lago client for usage reporting (optional).
    pub lago: Option<Arc<LagoClient>>,

    /// Stripe client for payments (optional).
    pub stripe: Option<Arc<StripeClient>>,
}

impl AppState {
    /// Create a new application state.
    #[must_use]
    pub fn new(store: Arc<RocksStore>, config: ServiceConfig) -> Self {
        // Create Lago client if configured
        let lago = config
            .lago_api_url
            .as_ref()
            .zip(config.lago_api_key.as_ref())
            .and_then(|(url, key)| {
                match LagoClient::new(url, key) {
                    Ok(client) => {
                        tracing::info!(lago_url = %url, "Lago integration enabled");
                        Some(Arc::new(client))
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to create Lago client");
                        None
                    }
                }
            });

        if lago.is_none() {
            tracing::warn!("Lago not configured - usage events will not be forwarded");
        }

        // Create Stripe client if configured
        let stripe = config.stripe_api_key.as_ref().and_then(|key| {
            match StripeClient::new(key, config.stripe_webhook_secret.clone()) {
                Ok(client) => {
                    tracing::info!("Stripe integration enabled");
                    Some(Arc::new(client))
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to create Stripe client");
                    None
                }
            }
        });

        if stripe.is_none() {
            tracing::warn!("Stripe not configured - payments will not be available");
        }

        Self {
            store,
            config,
            lago,
            stripe,
        }
    }

    /// Check if Lago is configured.
    #[must_use]
    pub fn has_lago(&self) -> bool {
        self.lago.is_some()
    }

    /// Check if Stripe is configured.
    #[must_use]
    pub fn has_stripe(&self) -> bool {
        self.stripe.is_some()
    }
}
