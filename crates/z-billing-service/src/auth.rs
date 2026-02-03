//! Authentication middleware and extractors.
//!
//! This module provides extractors for:
//! - `AuthUser` - End-user authentication via ZID JWT
//! - `ServiceAuth` - Service-to-service authentication via API key
//! - `AdminAuth` - Admin authentication for privileged endpoints

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use z_billing_core::UserId;

use crate::error::ApiError;
use crate::state::AppState;

// ============================================================================
// Constants
// ============================================================================

/// How long to cache JWKS keys before refreshing.
const JWKS_CACHE_DURATION: Duration = Duration::from_secs(3600); // 1 hour

/// Timeout for JWKS fetch requests.
const JWKS_FETCH_TIMEOUT: Duration = Duration::from_secs(10);

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

            // Allow test tokens in testing only.
            // This bypass is gated behind #[cfg(test)] or the "test-auth" feature
            // to ensure it is never active in production builds.
            #[cfg(any(test, feature = "test-auth"))]
            if let Some(user_id_str) = token.strip_prefix("test-token:") {
                let user_id = user_id_str
                    .parse::<UserId>()
                    .map_err(|_| ApiError::Unauthorized)?;

                return Ok(AuthUser {
                    user_id,
                    subject: user_id_str.to_string(),
                });
            }

            // Validate JWT against JWKS
            let claims = validate_jwt(token, state).await?;

            let user_id = claims
                .sub
                .parse::<UserId>()
                .map_err(|_| ApiError::Unauthorized)?;

            Ok(AuthUser {
                user_id,
                subject: claims.sub,
            })
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

/// Admin authentication via API key with admin scope.
///
/// Used for admin-only endpoints like adding credits manually.
/// Requires the `X-Admin-Key` header to match the configured admin key.
#[derive(Debug, Clone)]
pub struct AdminAuth {
    /// Admin identifier (for audit logging).
    pub admin_id: String,
}

impl FromRequestParts<Arc<AppState>> for AdminAuth {
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
            // Check for X-Admin-Key header
            let admin_key = parts
                .headers
                .get("x-admin-key")
                .and_then(|v| v.to_str().ok())
                .ok_or(ApiError::Unauthorized)?;

            // Validate against configured admin API key
            let expected_key = state
                .config
                .admin_api_key
                .as_ref()
                .ok_or(ApiError::Unauthorized)?;

            if admin_key != expected_key {
                return Err(ApiError::Unauthorized);
            }

            // Extract admin identifier from header if provided
            let admin_id = parts
                .headers
                .get("x-admin-id")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("admin")
                .to_string();

            tracing::info!(admin_id = %admin_id, "Admin authenticated");

            Ok(AdminAuth { admin_id })
        })
    }
}

/// JWT claims structure for ZID tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject (user ID).
    pub sub: String,
    /// Audience (can be string or array).
    #[serde(default)]
    pub aud: Option<serde_json::Value>,
    /// Issuer.
    pub iss: String,
    /// Expiration time.
    pub exp: i64,
    /// Issued at.
    pub iat: i64,
    /// Key ID (from header, not claims).
    #[serde(skip)]
    pub kid: Option<String>,
}

// ============================================================================
// JWKS Client and JWT Validation
// ============================================================================

/// JWKS (JSON Web Key Set) response structure.
#[derive(Debug, Clone, Deserialize)]
pub struct Jwks {
    /// List of JWK keys.
    pub keys: Vec<Jwk>,
}

/// Single JSON Web Key.
#[derive(Debug, Clone, Deserialize)]
pub struct Jwk {
    /// Key type (e.g., "RSA").
    pub kty: String,
    /// Key ID.
    pub kid: Option<String>,
    /// Algorithm (e.g., "RS256").
    pub alg: Option<String>,
    /// RSA public key modulus (base64url encoded).
    pub n: Option<String>,
    /// RSA public key exponent (base64url encoded).
    pub e: Option<String>,
    /// Key use (e.g., "sig" for signature).
    #[serde(rename = "use")]
    pub key_use: Option<String>,
}

/// JWKS cache entry.
struct JwksCache {
    /// Reusable HTTP client for JWKS fetches.
    /// Creating a new client per request is expensive; reusing it allows
    /// connection pooling and reduces overhead.
    client: reqwest::Client,
    /// Cached keys mapped by kid.
    keys: HashMap<String, DecodingKey>,
    /// Default key (for tokens without kid).
    default_key: Option<DecodingKey>,
    /// When the cache was last updated.
    last_updated: Instant,
}

impl JwksCache {
    fn new() -> Self {
        // Build client once at initialization; this is called lazily on first use
        let client = reqwest::Client::builder()
            .timeout(JWKS_FETCH_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            keys: HashMap::new(),
            default_key: None,
            // Force initial fetch by setting last_updated to epoch (or as far back as possible)
            last_updated: Instant::now()
                .checked_sub(JWKS_CACHE_DURATION)
                .unwrap_or_else(Instant::now),
        }
    }

    fn is_expired(&self) -> bool {
        self.last_updated.elapsed() >= JWKS_CACHE_DURATION
    }
}

/// Global JWKS cache (lazily initialized).
static JWKS_CACHE: std::sync::OnceLock<RwLock<JwksCache>> = std::sync::OnceLock::new();

fn get_jwks_cache() -> &'static RwLock<JwksCache> {
    JWKS_CACHE.get_or_init(|| RwLock::new(JwksCache::new()))
}

/// Validate a JWT token against the JWKS.
async fn validate_jwt(token: &str, state: &AppState) -> Result<JwtClaims, ApiError> {
    // Decode the header to get the key ID
    let header = decode_header(token).map_err(|e| {
        tracing::debug!(error = %e, "Failed to decode JWT header");
        ApiError::Unauthorized
    })?;

    let kid = header.kid.clone();

    // Get the decoding key from cache or fetch JWKS
    let decoding_key = get_decoding_key(kid.as_deref(), state).await?;

    // Set up validation
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[&state.config.auth_audience]);
    validation.set_issuer(&[&state.config.auth_base_url]);

    // Decode and validate the token
    let token_data = decode::<JwtClaims>(token, &decoding_key, &validation).map_err(|e| {
        tracing::debug!(error = %e, "JWT validation failed");
        ApiError::Unauthorized
    })?;

    Ok(token_data.claims)
}

/// Get a decoding key from cache or fetch from JWKS endpoint.
async fn get_decoding_key(kid: Option<&str>, state: &AppState) -> Result<DecodingKey, ApiError> {
    let cache = get_jwks_cache();

    // Check cache first
    {
        let cache_read = cache.read().await;
        if !cache_read.is_expired() {
            if let Some(kid) = kid {
                if let Some(key) = cache_read.keys.get(kid) {
                    return Ok(key.clone());
                }
            } else if let Some(key) = &cache_read.default_key {
                return Ok(key.clone());
            }
        }
    }

    // Cache miss or expired - fetch JWKS
    let jwks = fetch_jwks(state).await?;

    // Update cache
    let mut cache_write = cache.write().await;
    cache_write.keys.clear();
    cache_write.default_key = None;
    cache_write.last_updated = Instant::now();

    for jwk in &jwks.keys {
        if let Some(decoding_key) = jwk_to_decoding_key(jwk) {
            if let Some(ref key_kid) = jwk.kid {
                cache_write.keys.insert(key_kid.clone(), decoding_key.clone());
            }
            // Set first key as default
            if cache_write.default_key.is_none() {
                cache_write.default_key = Some(decoding_key);
            }
        }
    }

    // Return the requested key
    if let Some(kid) = kid {
        cache_write
            .keys
            .get(kid)
            .cloned()
            .ok_or(ApiError::Unauthorized)
    } else {
        cache_write.default_key.clone().ok_or(ApiError::Unauthorized)
    }
}

/// Fetch JWKS from the auth provider.
///
/// Uses the cached HTTP client from `JwksCache` to enable connection reuse.
async fn fetch_jwks(state: &AppState) -> Result<Jwks, ApiError> {
    let jwks_url = format!("{}/.well-known/jwks.json", state.config.auth_base_url);

    tracing::debug!(url = %jwks_url, "Fetching JWKS");

    // Get the cached client for connection reuse
    let cache = get_jwks_cache();
    let client = {
        let cache_read = cache.read().await;
        cache_read.client.clone()
    };

    let response = client.get(&jwks_url).send().await.map_err(|e| {
        tracing::error!(error = %e, url = %jwks_url, "Failed to fetch JWKS");
        ApiError::ExternalService("Failed to fetch authentication keys".into())
    })?;

    if !response.status().is_success() {
        tracing::error!(
            status = %response.status(),
            url = %jwks_url,
            "JWKS fetch returned non-success status"
        );
        return Err(ApiError::ExternalService(
            "Failed to fetch authentication keys".into(),
        ));
    }

    let jwks: Jwks = response.json().await.map_err(|e| {
        tracing::error!(error = %e, "Failed to parse JWKS response");
        ApiError::ExternalService("Failed to parse authentication keys".into())
    })?;

    tracing::info!(keys_count = %jwks.keys.len(), "JWKS fetched successfully");

    Ok(jwks)
}

/// Convert a JWK to a `DecodingKey`.
fn jwk_to_decoding_key(jwk: &Jwk) -> Option<DecodingKey> {
    // Only support RSA keys for now
    if jwk.kty != "RSA" {
        tracing::debug!(kty = %jwk.kty, "Skipping non-RSA JWK");
        return None;
    }

    let n = jwk.n.as_ref()?;
    let e = jwk.e.as_ref()?;

    DecodingKey::from_rsa_components(n, e).ok()
}
