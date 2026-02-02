//! Z-Billing Service - HTTP API for Z Credits and Billing
//!
//! This is the main entry point for the z-billing service.

use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use z_billing_service::{create_router, AppState, ServiceConfig};
use z_billing_store::RocksStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,z_billing=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Z-Billing Service");

    // Load configuration from environment
    let config = ServiceConfig::from_env();

    tracing::info!(
        listen_addr = %config.listen_addr,
        data_dir = %config.data_dir,
        lago_configured = %config.lago_api_url.is_some(),
        lago_org_id = ?config.lago_organization_id,
        stripe_configured = %config.stripe_api_key.is_some(),
        "Service configuration loaded"
    );

    // Initialize RocksDB store
    tracing::info!(path = %config.data_dir, "Opening RocksDB store");
    let store = Arc::new(RocksStore::open(&config.data_dir)?);

    // Build app state
    let state = AppState::new(store, config.clone());

    // Create the router
    let app = create_router(state);
    tracing::info!("Router configured with all API endpoints");

    // Start HTTP server
    tracing::info!(listen_addr = %config.listen_addr, "Starting HTTP server");
    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
