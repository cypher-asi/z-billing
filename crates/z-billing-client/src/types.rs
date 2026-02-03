//! Request and response types for the z-billing client.

use serde::{Deserialize, Serialize};

/// LLM usage event for convenient reporting.
#[derive(Debug, Clone, Serialize)]
pub struct LlmUsageEvent {
    /// Unique event ID for idempotency.
    pub event_id: String,
    /// User ID being charged.
    pub user_id: String,
    /// Agent ID that generated the usage (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// LLM provider (e.g., "anthropic", "openai").
    pub provider: String,
    /// Model name (e.g., "claude-3-5-sonnet").
    pub model: String,
    /// Number of input tokens.
    pub input_tokens: u64,
    /// Number of output tokens.
    pub output_tokens: u64,
    /// Additional metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Compute usage event.
#[derive(Debug, Clone, Serialize)]
pub struct ComputeUsageEvent {
    /// Unique event ID for idempotency.
    pub event_id: String,
    /// User ID being charged.
    pub user_id: String,
    /// Agent ID that generated the usage (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// CPU hours used.
    pub cpu_hours: f64,
    /// Memory GB-hours used.
    pub memory_gb_hours: f64,
    /// Additional metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Generic usage event request.
#[derive(Debug, Clone, Serialize)]
pub struct UsageRequest {
    /// Unique event ID for idempotency.
    pub event_id: String,
    /// User ID being charged.
    pub user_id: String,
    /// Agent ID that generated the usage (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Usage metric details.
    pub metric: UsageMetric,
    /// Pre-calculated cost in cents (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_cents: Option<i64>,
    /// Additional metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Usage metric variants.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UsageMetric {
    /// LLM token usage.
    LlmTokens {
        /// Provider name.
        provider: String,
        /// Model name.
        model: String,
        /// Input tokens.
        input_tokens: u64,
        /// Output tokens.
        output_tokens: u64,
    },
    /// Compute usage.
    Compute {
        /// CPU hours.
        cpu_hours: f64,
        /// Memory GB-hours.
        memory_gb_hours: f64,
    },
    /// API calls.
    ApiCalls {
        /// Endpoint name.
        endpoint: String,
        /// Number of calls.
        count: u64,
    },
}

/// Usage response from the API.
#[derive(Debug, Clone, Deserialize)]
pub struct UsageResponse {
    /// Whether the usage was processed successfully.
    pub success: bool,
    /// New balance after deduction.
    pub balance_cents: i64,
    /// Cost deducted.
    pub cost_cents: i64,
    /// Transaction ID.
    pub transaction_id: String,
}

/// Batch usage request.
#[derive(Debug, Clone, Serialize)]
pub struct BatchUsageRequest {
    /// List of usage events.
    pub events: Vec<UsageRequest>,
}

/// Batch usage response.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchUsageResponse {
    /// Results for each event.
    pub results: Vec<BatchUsageResult>,
    /// Total events processed.
    pub processed: usize,
    /// Total events failed.
    pub failed: usize,
}

/// Result for a single event in batch.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchUsageResult {
    /// Event ID.
    pub event_id: String,
    /// Whether successful.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Cost deducted (if successful).
    pub cost_cents: Option<i64>,
}

/// Balance check request.
#[derive(Debug, Clone, Serialize)]
pub struct CheckBalanceRequest {
    /// User ID to check.
    pub user_id: String,
    /// Required amount in cents.
    pub required_cents: i64,
}

/// Balance check response.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckBalanceResponse {
    /// Whether the user has sufficient balance.
    pub sufficient: bool,
    /// Current balance.
    pub balance_cents: i64,
    /// Required amount.
    pub required_cents: i64,
}

/// Balance response.
#[derive(Debug, Clone, Deserialize)]
pub struct BalanceResponse {
    /// Balance in cents.
    pub balance_cents: i64,
    /// Balance formatted.
    pub balance_formatted: String,
    /// Current plan.
    pub plan: String,
}

/// API error response.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorResponse {
    /// Error details.
    pub error: ApiErrorBody,
}

/// API error body.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    /// Error code.
    pub code: String,
    /// Error message.
    pub message: String,
    /// Additional details.
    pub details: Option<serde_json::Value>,
}
