//! Authentication middleware and extractors.
//!
//! This module provides extractors for:
//! - `AuthUser` - End-user authentication via ZID JWT
//! - `ServiceAuth` - Service-to-service authentication via API key

use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};

use z_billing_core::UserId;

use crate::error::ApiError;
use crate::state::AppState;

/// An authenticated user extracted from a ZID JWT token.
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// The user ID.
    pub user_id: UserId,
    /// The raw subject claim from the JWT.
    pub subject: String,
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = ApiError;

    fn from_request_parts<'life0, 'life1, 'async_trait>(
        parts: &'life0 mut Parts,
        _state: &'life1 Arc<AppState>,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<Self, Self::Rejection>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            // Extract the Authorization header
            let auth_header = parts
                .headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .ok_or(ApiError::Unauthorized)?;

            // Extract the Bearer token
            let token = auth_header
                .strip_prefix("Bearer ")
                .ok_or(ApiError::Unauthorized)?;

            // For now, we'll use a simple test token format: "test-token:<user-uuid>"
            // In production, this would validate against ZID JWKS
            if let Some(user_id_str) = token.strip_prefix("test-token:") {
                let user_id = user_id_str
                    .parse::<UserId>()
                    .map_err(|_| ApiError::Unauthorized)?;

                return Ok(AuthUser {
                    user_id,
                    subject: user_id_str.to_string(),
                });
            }

            // TODO: Implement real JWT validation against ZID JWKS
            // For now, reject non-test tokens
            Err(ApiError::Unauthorized)
        })
    }
}

/// Service authentication via API key.
///
/// Used for service-to-service requests (e.g., from aura-runtime).
#[derive(Debug, Clone)]
pub struct ServiceAuth {
    /// The service name or identifier.
    pub service_name: String,
}

impl FromRequestParts<Arc<AppState>> for ServiceAuth {
    type Rejection = ApiError;

    fn from_request_parts<'life0, 'life1, 'async_trait>(
        parts: &'life0 mut Parts,
        state: &'life1 Arc<AppState>,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<Self, Self::Rejection>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            // Check for X-API-Key header
            let api_key = parts
                .headers
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .ok_or(ApiError::Unauthorized)?;

            // Validate against configured service API key
            let expected_key = state
                .config
                .service_api_key
                .as_ref()
                .ok_or(ApiError::Unauthorized)?;

            if api_key != expected_key {
                return Err(ApiError::Unauthorized);
            }

            // Extract service name from header if provided
            let service_name = parts
                .headers
                .get("x-service-name")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("unknown")
                .to_string();

            Ok(ServiceAuth { service_name })
        })
    }
}

/// JWT claims structure for ZID tokens (for future implementation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject (user ID).
    pub sub: String,
    /// Audience.
    pub aud: String,
    /// Issuer.
    pub iss: String,
    /// Expiration time.
    pub exp: i64,
    /// Issued at.
    pub iat: i64,
}
