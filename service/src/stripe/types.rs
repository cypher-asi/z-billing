//! Stripe API types.

use serde::{Deserialize, Serialize};

/// Stripe customer object.
#[derive(Debug, Clone, Deserialize)]
pub struct Customer {
    /// Stripe customer ID.
    pub id: String,
    /// Customer email.
    #[serde(default)]
    pub email: Option<String>,
    /// Customer name.
    #[serde(default)]
    pub name: Option<String>,
    /// Metadata attached to the customer.
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Created timestamp (Unix).
    #[serde(default)]
    pub created: i64,
}

/// Stripe Checkout session object.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckoutSession {
    /// Session ID.
    pub id: String,
    /// Checkout URL to redirect the user to.
    #[serde(default)]
    pub url: Option<String>,
    /// Payment status.
    #[serde(default)]
    pub payment_status: Option<String>,
    /// Customer ID.
    #[serde(default)]
    pub customer: Option<String>,
    /// Total amount in cents.
    #[serde(default)]
    pub amount_total: Option<i64>,
    /// Client reference ID (our `user_id`).
    #[serde(default)]
    pub client_reference_id: Option<String>,
    /// Session status.
    #[serde(default)]
    pub status: Option<String>,
    /// Payment intent ID.
    #[serde(default)]
    pub payment_intent: Option<String>,
    /// Metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Stripe `PaymentIntent` object.
#[derive(Debug, Clone, Deserialize)]
pub struct PaymentIntent {
    /// Payment intent ID.
    pub id: String,
    /// Amount in cents.
    #[serde(default)]
    pub amount: i64,
    /// Currency (e.g., "usd").
    #[serde(default)]
    pub currency: String,
    /// Status (succeeded, pending, failed, etc.).
    #[serde(default)]
    pub status: String,
    /// Customer ID.
    #[serde(default)]
    pub customer: Option<String>,
    /// Created timestamp (Unix).
    #[serde(default)]
    pub created: i64,
    /// Metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
    /// Receipt email.
    #[serde(default)]
    pub receipt_email: Option<String>,
}

/// Stripe list response wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct StripeList<T> {
    /// Object type (always "list").
    pub object: String,
    /// Data items.
    pub data: Vec<T>,
    /// Whether there are more items.
    pub has_more: bool,
    /// URL for the list endpoint.
    #[serde(default)]
    pub url: Option<String>,
}

/// Stripe webhook event.
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookEvent {
    /// Event ID.
    pub id: String,
    /// Event type (e.g., "checkout.session.completed").
    #[serde(rename = "type")]
    pub event_type: String,
    /// Event data.
    pub data: WebhookEventData,
    /// Created timestamp (Unix).
    pub created: i64,
}

/// Webhook event data container.
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookEventData {
    /// The event object.
    pub object: serde_json::Value,
}

/// Response for listing payments.
#[derive(Debug, Serialize)]
pub struct PaymentResponse {
    /// Payment intent ID.
    pub id: String,
    /// Amount in cents.
    pub amount_cents: i64,
    /// Amount formatted as dollars.
    pub amount_formatted: String,
    /// Currency.
    pub currency: String,
    /// Status.
    pub status: String,
    /// Description.
    pub description: Option<String>,
    /// Created timestamp (ISO 8601).
    pub created_at: String,
}

impl From<&PaymentIntent> for PaymentResponse {
    #[allow(clippy::cast_precision_loss)]
    fn from(pi: &PaymentIntent) -> Self {
        Self {
            id: pi.id.clone(),
            amount_cents: pi.amount,
            amount_formatted: format!("${:.2}", pi.amount as f64 / 100.0),
            currency: pi.currency.clone(),
            status: pi.status.clone(),
            description: pi.description.clone(),
            created_at: chrono::DateTime::from_timestamp(pi.created, 0)
                .map_or_else(|| pi.created.to_string(), |dt| dt.to_rfc3339()),
        }
    }
}

/// Checkout line item for creating sessions.
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutLineItem {
    /// Price data for the line item.
    pub price_data: PriceData,
    /// Quantity.
    pub quantity: i64,
}

/// Price data for checkout.
#[derive(Debug, Clone, Serialize)]
pub struct PriceData {
    /// Currency (e.g., "usd").
    pub currency: String,
    /// Product data.
    pub product_data: ProductData,
    /// Unit amount in cents.
    pub unit_amount: i64,
}

/// Product data for checkout.
#[derive(Debug, Clone, Serialize)]
pub struct ProductData {
    /// Product name.
    pub name: String,
    /// Product description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Stripe API error response.
#[derive(Debug, Clone, Deserialize)]
pub struct StripeErrorResponse {
    /// Error details.
    pub error: StripeErrorDetail,
}

/// Stripe error detail.
#[derive(Debug, Clone, Deserialize)]
pub struct StripeErrorDetail {
    /// Error type.
    #[serde(rename = "type")]
    pub error_type: String,
    /// Error message.
    pub message: String,
    /// Error code.
    #[serde(default)]
    pub code: Option<String>,
    /// Parameter that caused the error.
    #[serde(default)]
    pub param: Option<String>,
}
