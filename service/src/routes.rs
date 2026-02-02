//! Router configuration.
//!
//! This module sets up the Axum router with all routes and middleware.

use std::sync::Arc;
use std::time::Duration;

use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::handlers::{accounts, credits, health, usage, webhooks};
use crate::state::AppState;

/// Create the service router with all routes and middleware.
///
/// # Routes
///
/// ## Public
/// - `GET /health` - Health check
///
/// ## Accounts (ZID JWT auth)
/// - `POST /v1/accounts` - Create/register account
/// - `GET /v1/accounts/me` - Get current user's account
///
/// ## Credits (ZID JWT auth)
/// - `GET /v1/credits/balance` - Get current balance
/// - `GET /v1/credits/transactions` - List transaction history
/// - `POST /v1/credits/purchase` - Initiate credit purchase
/// - `POST /v1/credits/auto-refill` - Configure auto-refill
///
/// ## Usage (Service API Key auth)
/// - `POST /v1/usage` - Report usage event
/// - `POST /v1/usage/batch` - Report multiple usage events
///
/// ## Webhooks (Signature verification)
/// - `POST /webhooks/stripe` - Stripe webhooks
/// - `POST /webhooks/lago` - Lago webhooks
pub fn create_router(state: AppState) -> Router {
    // Extract config values before moving state
    let cors_origins = state.config.cors_origins.clone();
    let max_body_bytes = state.config.max_body_bytes;
    let request_timeout_seconds = state.config.request_timeout_seconds;

    // Build CORS layer
    let cors = build_cors_layer(&cors_origins);

    let state = Arc::new(state);

    Router::new()
        // Health (public)
        .route("/health", get(health::health))
        // Accounts
        .route("/v1/accounts", post(accounts::create_account))
        .route("/v1/accounts/me", get(accounts::get_account))
        .route("/v1/accounts/me", delete(accounts::delete_account))
        // Credits
        .route("/v1/credits/balance", get(credits::get_balance))
        .route("/v1/credits/transactions", get(credits::list_transactions))
        .route("/v1/credits/purchase", post(credits::purchase_credits))
        .route(
            "/v1/credits/auto-refill",
            post(credits::configure_auto_refill),
        )
        .route("/v1/credits/add", post(credits::admin_add_credits))
        // Payments (Stripe history)
        .route("/v1/payments", get(credits::list_payments))
        // Usage (service auth)
        .route("/v1/usage", post(usage::report_usage))
        .route("/v1/usage/batch", post(usage::report_usage_batch))
        .route("/v1/usage/check", post(usage::check_balance))
        // Webhooks
        .route("/webhooks/stripe", post(webhooks::stripe_webhook))
        .route("/webhooks/lago", post(webhooks::lago_webhook))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(max_body_bytes))
        .layer(TimeoutLayer::new(Duration::from_secs(
            request_timeout_seconds,
        )))
        .with_state(state)
}

/// Build the CORS layer from configured origins.
fn build_cors_layer(origins: &[String]) -> CorsLayer {
    if origins.iter().any(|o| o == "*") {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        let origins: Vec<_> = origins.iter().filter_map(|o| o.parse().ok()).collect();

        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(Any)
            .allow_headers(Any)
    }
}
