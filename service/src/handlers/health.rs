//! Health check handlers.

use axum::Json;
use serde::Serialize;

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Service status.
    pub status: String,
    /// Service name.
    pub service: String,
    /// Service version.
    pub version: String,
}

/// Health check endpoint.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        service: "z-billing".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
