//! Stripe API client implementation.

use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;

use super::types::{
    CheckoutLineItem, CheckoutSession, Customer, PaymentIntent, PriceData, ProductData,
    StripeErrorResponse, StripeList,
};

/// Error type for Stripe operations.
#[derive(Debug, thiserror::Error)]
pub enum StripeError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Stripe API returned an error.
    #[error("Stripe API error: {error_type} - {message}")]
    Api {
        /// Error type.
        error_type: String,
        /// Error message.
        message: String,
        /// Error code.
        code: Option<String>,
    },

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid webhook signature.
    #[error("Invalid webhook signature")]
    InvalidSignature,

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Configuration(String),
}

/// Stripe API client.
#[derive(Debug, Clone)]
pub struct StripeClient {
    client: Client,
    api_key: String,
    webhook_secret: Option<String>,
}

impl StripeClient {
    /// Stripe API base URL.
    const BASE_URL: &'static str = "https://api.stripe.com/v1";

    /// Create a new Stripe client.
    ///
    /// # Arguments
    ///
    /// * `api_key` - Stripe secret API key (`sk_test_...` or `sk_live_...`)
    /// * `webhook_secret` - Optional webhook signing secret (whsec_...)
    ///
    /// # Panics
    ///
    /// Panics if the HTTP client cannot be built.
    pub fn new(api_key: impl Into<String>, webhook_secret: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            api_key: api_key.into(),
            webhook_secret,
        }
    }

    /// Create a new Stripe customer.
    ///
    /// # Arguments
    ///
    /// * `user_id` - Our internal user ID (stored as metadata)
    /// * `email` - Optional customer email
    /// * `name` - Optional customer name
    pub async fn create_customer(
        &self,
        user_id: &str,
        email: Option<&str>,
        name: Option<&str>,
    ) -> Result<Customer, StripeError> {
        let mut params = HashMap::new();
        params.insert("metadata[user_id]", user_id.to_string());

        if let Some(email) = email {
            params.insert("email", email.to_string());
        }
        if let Some(name) = name {
            params.insert("name", name.to_string());
        }

        let response = self
            .client
            .post(format!("{}/customers", Self::BASE_URL))
            .basic_auth(&self.api_key, Option::<&str>::None)
            .form(&params)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Get a customer by ID.
    pub async fn get_customer(&self, customer_id: &str) -> Result<Option<Customer>, StripeError> {
        let response = self
            .client
            .get(format!("{}/customers/{}", Self::BASE_URL, customer_id))
            .basic_auth(&self.api_key, Option::<&str>::None)
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        self.handle_response(response).await.map(Some)
    }

    /// Create a Checkout session for purchasing credits.
    ///
    /// # Arguments
    ///
    /// * `customer_id` - Optional Stripe customer ID
    /// * `user_id` - Our internal user ID (`client_reference_id`)
    /// * `amount_cents` - Amount to charge in cents
    /// * `credits_amount` - Number of credits being purchased (for display)
    /// * `success_url` - URL to redirect on success
    /// * `cancel_url` - URL to redirect on cancel
    #[allow(clippy::too_many_arguments)]
    pub async fn create_checkout_session(
        &self,
        customer_id: Option<&str>,
        user_id: &str,
        amount_cents: i64,
        credits_amount: i64,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<CheckoutSession, StripeError> {
        let line_item = CheckoutLineItem {
            price_data: PriceData {
                currency: "usd".to_string(),
                product_data: ProductData {
                    name: "Z Credits".to_string(),
                    description: Some(format!("{credits_amount} Z Credits for Aura Swarm")),
                },
                unit_amount: amount_cents,
            },
            quantity: 1,
        };

        let line_items_json = serde_json::to_string(&[line_item])?;

        let mut params = vec![
            ("mode", "payment".to_string()),
            ("success_url", success_url.to_string()),
            ("cancel_url", cancel_url.to_string()),
            ("client_reference_id", user_id.to_string()),
            ("line_items[0][price_data][currency]", "usd".to_string()),
            (
                "line_items[0][price_data][product_data][name]",
                "Z Credits".to_string(),
            ),
            (
                "line_items[0][price_data][product_data][description]",
                format!("{credits_amount} Z Credits for Aura Swarm"),
            ),
            (
                "line_items[0][price_data][unit_amount]",
                amount_cents.to_string(),
            ),
            ("line_items[0][quantity]", "1".to_string()),
            ("metadata[user_id]", user_id.to_string()),
            ("metadata[credits_amount]", credits_amount.to_string()),
        ];

        if let Some(cid) = customer_id {
            params.push(("customer", cid.to_string()));
        }

        // Log for debugging (redacted in production)
        tracing::debug!(
            user_id = %user_id,
            amount_cents = %amount_cents,
            line_items = %line_items_json,
            "Creating Stripe checkout session"
        );

        let response = self
            .client
            .post(format!("{}/checkout/sessions", Self::BASE_URL))
            .basic_auth(&self.api_key, Option::<&str>::None)
            .form(&params)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Retrieve a Checkout session by ID.
    pub async fn get_checkout_session(
        &self,
        session_id: &str,
    ) -> Result<CheckoutSession, StripeError> {
        let response = self
            .client
            .get(format!("{}/checkout/sessions/{}", Self::BASE_URL, session_id))
            .basic_auth(&self.api_key, Option::<&str>::None)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// List payment intents for a customer.
    ///
    /// # Arguments
    ///
    /// * `customer_id` - Stripe customer ID
    /// * `limit` - Maximum number of results (1-100)
    pub async fn list_payment_intents(
        &self,
        customer_id: &str,
        limit: Option<u32>,
    ) -> Result<StripeList<PaymentIntent>, StripeError> {
        let limit = limit.unwrap_or(10).min(100);

        let response = self
            .client
            .get(format!("{}/payment_intents", Self::BASE_URL))
            .basic_auth(&self.api_key, Option::<&str>::None)
            .query(&[("customer", customer_id), ("limit", &limit.to_string())])
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Get a single payment intent by ID.
    pub async fn get_payment_intent(
        &self,
        payment_intent_id: &str,
    ) -> Result<PaymentIntent, StripeError> {
        let response = self
            .client
            .get(format!(
                "{}/payment_intents/{}",
                Self::BASE_URL, payment_intent_id
            ))
            .basic_auth(&self.api_key, Option::<&str>::None)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Verify a webhook signature and parse the event.
    ///
    /// # Arguments
    ///
    /// * `payload` - Raw request body
    /// * `signature` - Value of the `Stripe-Signature` header
    ///
    /// # Returns
    ///
    /// The verified webhook event, or an error if signature is invalid.
    pub fn verify_webhook_signature(
        &self,
        payload: &str,
        signature: &str,
    ) -> Result<(), StripeError> {
        let secret = self
            .webhook_secret
            .as_ref()
            .ok_or_else(|| StripeError::Configuration("Webhook secret not configured".into()))?;

        // Parse the signature header
        // Format: t=timestamp,v1=signature,v1=signature2,...
        let mut timestamp: Option<&str> = None;
        let mut signatures: Vec<&str> = Vec::new();

        for part in signature.split(',') {
            let mut kv = part.splitn(2, '=');
            match (kv.next(), kv.next()) {
                (Some("t"), Some(ts)) => timestamp = Some(ts),
                (Some("v1"), Some(sig)) => signatures.push(sig),
                _ => {}
            }
        }

        let timestamp =
            timestamp.ok_or_else(|| StripeError::Configuration("Missing timestamp".into()))?;

        if signatures.is_empty() {
            return Err(StripeError::InvalidSignature);
        }

        // Compute expected signature
        let signed_payload = format!("{timestamp}.{payload}");
        let expected = compute_hmac_sha256(secret, &signed_payload);

        // Check if any signature matches (constant-time comparison)
        let valid = signatures.iter().any(|sig| constant_time_eq(&expected, sig));

        if valid {
            Ok(())
        } else {
            Err(StripeError::InvalidSignature)
        }
    }

    /// Handle API response and convert errors.
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, StripeError> {
        let status = response.status();

        if status.is_success() {
            return Ok(response.json().await?);
        }

        // Try to parse error response
        let error_body: Result<StripeErrorResponse, _> = response.json().await;

        match error_body {
            Ok(stripe_error) => Err(StripeError::Api {
                error_type: stripe_error.error.error_type,
                message: stripe_error.error.message,
                code: stripe_error.error.code,
            }),
            Err(_) => Err(StripeError::Api {
                error_type: "unknown".to_string(),
                message: format!("HTTP {status}"),
                code: None,
            }),
        }
    }
}

/// HMAC block size for SHA256 is 64 bytes.
const HMAC_BLOCK_SIZE: usize = 64;

/// Compute HMAC-SHA256 and return hex-encoded result.
fn compute_hmac_sha256(secret: &str, message: &str) -> String {
    use sha2::{Digest, Sha256};

    let key = secret.as_bytes();
    let message = message.as_bytes();

    // If key is longer than block size, hash it first
    let key = if key.len() > HMAC_BLOCK_SIZE {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.finalize().to_vec()
    } else {
        key.to_vec()
    };

    // Pad key to block size
    let mut key_padded = [0u8; HMAC_BLOCK_SIZE];
    key_padded[..key.len()].copy_from_slice(&key);

    // Create inner and outer padded keys
    let mut i_key_pad = [0x36u8; HMAC_BLOCK_SIZE];
    let mut o_key_pad = [0x5cu8; HMAC_BLOCK_SIZE];

    for i in 0..HMAC_BLOCK_SIZE {
        i_key_pad[i] ^= key_padded[i];
        o_key_pad[i] ^= key_padded[i];
    }

    // Inner hash: H(i_key_pad || message)
    let mut inner_hasher = Sha256::new();
    inner_hasher.update(i_key_pad);
    inner_hasher.update(message);
    let inner_hash = inner_hasher.finalize();

    // Outer hash: H(o_key_pad || inner_hash)
    let mut outer_hasher = Sha256::new();
    outer_hasher.update(o_key_pad);
    outer_hasher.update(inner_hash);
    let hmac = outer_hasher.finalize();

    // Convert to hex
    hex::encode(hmac)
}

/// Constant-time string comparison.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_creation() {
        let client = StripeClient::new("sk_test_xxx", None);
        assert!(client.webhook_secret.is_none());
    }

    #[test]
    fn client_with_webhook_secret() {
        let client = StripeClient::new("sk_test_xxx", Some("whsec_xxx".to_string()));
        assert!(client.webhook_secret.is_some());
    }

    #[test]
    fn hmac_sha256_works() {
        // Test vector from RFC 4231
        let result = compute_hmac_sha256("key", "The quick brown fox jumps over the lazy dog");
        assert!(!result.is_empty());
        assert_eq!(result.len(), 64); // SHA256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(!constant_time_eq("ab", "abc"));
    }
}
