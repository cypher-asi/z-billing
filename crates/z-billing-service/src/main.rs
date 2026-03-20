//! Z-Billing Service - HTTP API for Z Credits and Billing
//!
//! This is the main entry point for the z-billing service.
//! Supports PostgreSQL (preferred) or RocksDB as the storage backend.

use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use z_billing_service::{create_router, AppState, ServiceConfig};

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
        database = if config.database_url.is_some() { "postgresql" } else { "rocksdb" },
        lago_configured = %config.lago_api_url.is_some(),
        stripe_configured = %config.stripe_api_key.is_some(),
        "Service configuration loaded"
    );

    // Initialize store based on configuration
    let store: Arc<dyn z_billing_store::Store> = if let Some(ref database_url) = config.database_url
    {
        tracing::info!("Connecting to PostgreSQL");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(20)
            .connect(database_url)
            .await?;

        tracing::info!("Running database migrations");
        sqlx::migrate!("../z-billing-store/migrations")
            .run(&pool)
            .await?;
        tracing::info!("Migrations complete");

        Arc::new(z_billing_store::PgStore::new(pool))
    } else {
        #[cfg(feature = "rocksdb-backend")]
        {
            tracing::info!(path = %config.data_dir, "Opening RocksDB store");
            Arc::new(z_billing_store::RocksStore::open(&config.data_dir)?)
        }
        #[cfg(not(feature = "rocksdb-backend"))]
        {
            return Err("DATABASE_URL is required (RocksDB backend not compiled)".into());
        }
    };

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
