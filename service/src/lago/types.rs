//! Lago API types.

use serde::{Deserialize, Serialize};

/// Lago customer creation request.
#[derive(Debug, Clone, Serialize)]
pub struct CreateCustomerRequest {
    /// Customer data.
    pub customer: CustomerInput,
}

/// Customer input for Lago.
#[derive(Debug, Clone, Serialize)]
pub struct CustomerInput {
    /// External ID (our `user_id`).
    pub external_id: String,
    /// Customer name.
    pub name: String,
    /// Customer email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Billing configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_configuration: Option<BillingConfiguration>,
    /// Metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Vec<MetadataInput>>,
}

/// Billing configuration.
#[derive(Debug, Clone, Serialize)]
pub struct BillingConfiguration {
    /// Payment provider (stripe).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_provider: Option<String>,
    /// Provider customer ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_customer_id: Option<String>,
    /// Sync with provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_with_provider: Option<bool>,
}

/// Metadata input.
#[derive(Debug, Clone, Serialize)]
pub struct MetadataInput {
    /// Key.
    pub key: String,
    /// Value.
    pub value: String,
}

/// Lago customer response.
#[derive(Debug, Clone, Deserialize)]
pub struct CustomerResponse {
    /// Customer data.
    pub customer: Customer,
}

/// Lago customer.
#[derive(Debug, Clone, Deserialize)]
pub struct Customer {
    /// Lago internal ID.
    pub lago_id: String,
    /// External ID (our `user_id`).
    pub external_id: String,
    /// Customer name.
    pub name: Option<String>,
    /// Customer email.
    pub email: Option<String>,
    /// Created timestamp.
    pub created_at: String,
}

/// Subscription creation request.
#[derive(Debug, Clone, Serialize)]
pub struct CreateSubscriptionRequest {
    /// Subscription data.
    pub subscription: SubscriptionInput,
}

/// Subscription input.
#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionInput {
    /// External customer ID.
    pub external_customer_id: String,
    /// Plan code.
    pub plan_code: String,
    /// External ID for the subscription (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// Subscription name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Billing time (calendar or anniversary).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_time: Option<String>,
}

/// Subscription response.
#[derive(Debug, Clone, Deserialize)]
pub struct SubscriptionResponse {
    /// Subscription data.
    pub subscription: Subscription,
}

/// Lago subscription.
#[derive(Debug, Clone, Deserialize)]
pub struct Subscription {
    /// Lago internal ID.
    pub lago_id: String,
    /// External ID.
    pub external_id: String,
    /// Lago customer ID.
    pub lago_customer_id: String,
    /// External customer ID.
    pub external_customer_id: String,
    /// Plan code.
    pub plan_code: String,
    /// Status.
    pub status: String,
    /// Started at.
    pub started_at: Option<String>,
    /// Ending at.
    pub ending_at: Option<String>,
    /// Created at.
    pub created_at: String,
}

/// Usage event request.
#[derive(Debug, Clone, Serialize)]
pub struct CreateEventRequest {
    /// Event data.
    pub event: EventInput,
}

/// Event input for Lago.
#[derive(Debug, Clone, Serialize)]
pub struct EventInput {
    /// Unique transaction ID for idempotency.
    pub transaction_id: String,
    /// External customer ID.
    pub external_customer_id: String,
    /// Billable metric code.
    pub code: String,
    /// Timestamp (ISO 8601).
    pub timestamp: String,
    /// Event properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,
    /// External subscription ID (if multiple subscriptions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_subscription_id: Option<String>,
}

/// Event response.
#[derive(Debug, Clone, Deserialize)]
pub struct EventResponse {
    /// Event data.
    pub event: Event,
}

/// Lago event.
#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    /// Lago internal ID.
    pub lago_id: String,
    /// Transaction ID.
    pub transaction_id: String,
    /// Customer ID.
    #[serde(default)]
    pub lago_customer_id: Option<String>,
    /// External customer ID.
    #[serde(default)]
    pub external_customer_id: Option<String>,
    /// Metric code.
    pub code: String,
    /// Timestamp.
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Properties.
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
}

/// Lago API error response.
#[derive(Debug, Clone, Deserialize)]
pub struct LagoErrorResponse {
    /// Status code.
    pub status: u16,
    /// Error type.
    pub error: String,
    /// Error code.
    #[serde(default)]
    pub code: Option<String>,
    /// Error details.
    #[serde(default)]
    pub error_details: Option<serde_json::Value>,
}

/// Billable metric codes we use.
pub mod metrics {
    /// CPU hours metric.
    pub const CPU_HOURS: &str = "cpu_hours";
    /// Memory GB hours metric.
    pub const MEMORY_GB_HOURS: &str = "memory_gb_hours";
    /// LLM input tokens metric.
    pub const LLM_INPUT_TOKENS: &str = "llm_input_tokens";
    /// LLM output tokens metric.
    pub const LLM_OUTPUT_TOKENS: &str = "llm_output_tokens";
}

/// Plan codes we use.
pub mod plans {
    /// Free plan.
    pub const FREE: &str = "free";
    /// Standard plan ($20/month, 2500 credits).
    pub const STANDARD: &str = "standard";
    /// Pro plan ($50/month, 6000 credits).
    pub const PRO: &str = "pro";
    /// Enterprise plan (custom).
    pub const ENTERPRISE: &str = "enterprise";
}
