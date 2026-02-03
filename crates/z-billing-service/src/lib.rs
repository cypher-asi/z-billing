//! Z-Billing HTTP API Service.
//!
//! This crate provides the HTTP API for the z-billing service, including:
//!
//! - Account management
//! - Credit balance and transactions
//! - Usage event ingestion
//! - Stripe/Lago webhooks
//!
//! # Authentication
//!
//! The service supports two authentication methods:
//!
//! 1. **ZID JWT tokens** - For end-user requests (dashboard, etc.)
//! 2. **Service API keys** - For service-to-service requests (aura-runtime, etc.)

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
// Allow some pedantic lints that are noisy for Axum handler functions
#![allow(clippy::missing_errors_doc)] // Axum handlers all return Result
#![allow(clippy::unused_async)] // Webhook handlers need async for consistency

pub mod auth;
pub mod config;
pub mod crypto;
pub mod error;
pub mod handlers;
pub mod lago;
pub mod routes;
pub mod state;
pub mod stripe;

pub use config::ServiceConfig;
pub use error::ApiError;
pub use lago::{CustomerInput, EventInput, LagoClient, SubscriptionInput};
pub use routes::create_router;
pub use state::AppState;
pub use stripe::{StripeClient, StripeError};
