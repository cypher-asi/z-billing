//! Router configuration.
//!
//! This module sets up the Axum router with all routes and middleware.

use std::sync::Arc;
use std::time::Duration;

use axum::routing::{delete, get, post};
use axum::Router;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::handlers::{accounts, credits, health, usage, webhooks};
use crate::state::AppState;

// ============================================================================
// Concurrency Limiting Constants
// ============================================================================

/// Maximum concurrent requests for usage endpoints.
/// This prevents overload from high-volume usage reporting.
const USAGE_MAX_CONCURRENT_REQUESTS: usize = 100;

/// Maximum concurrent requests for general API endpoints.
const API_MAX_CONCURRENT_REQUESTS: usize = 50;

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
/// ## Usage (Service API Key auth, rate-limited)
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

    // Create concurrency-limited usage routes
    // Usage endpoints handle high-volume traffic from services, so they have
    // a higher concurrency limit but are still protected from overload.
    let usage_routes = Router::new()
        .route("/", post(usage::report_usage))
        .route("/batch", post(usage::report_usage_batch))
        .route("/check", post(usage::check_balance))
        .layer(ConcurrencyLimitLayer::new(USAGE_MAX_CONCURRENT_REQUESTS));

    // Create concurrency-limited API routes
    let api_routes = Router::new()
        // Accounts
        .route("/accounts", post(accounts::create_account))
        .route("/accounts/me", get(accounts::get_account))
        .route("/accounts/me", delete(accounts::delete_account))
        // Credits
        .route("/credits/balance", get(credits::get_balance))
        .route("/credits/transactions", get(credits::list_transactions))
        .route("/credits/purchase", post(credits::purchase_credits))
        .route("/credits/auto-refill", post(credits::configure_auto_refill))
        .route("/credits/add", post(credits::admin_add_credits))
        // Payments (Stripe history)
        .route("/payments", get(credits::list_payments))
        // Usage routes (with their own concurrency limit)
        .nest("/usage", usage_routes)
        .layer(ConcurrencyLimitLayer::new(API_MAX_CONCURRENT_REQUESTS));

    Router::new()
        // Health (public, no rate limit)
        .route("/health", get(health::health))
        // API v1 routes (rate limited)
        .nest("/v1", api_routes)
        // Webhooks (no rate limit - controlled by external services)
        .route("/webhooks/stripe", post(webhooks::stripe_webhook))
        .route("/webhooks/lago", post(webhooks::lago_webhook))
        // Global middleware
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
