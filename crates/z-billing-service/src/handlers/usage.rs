//! Usage event handlers.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use z_billing_core::{
    AgentId, CreditTransaction, LlmProvider, TokenDirection, UsageEvent, UsageMetric, UsageSource,
    UserId,
};
use z_billing_store::{RocksStore, Store};

use crate::auth::ServiceAuth;
use crate::error::ApiError;
use crate::state::AppState;
use crate::stripe::StripeClient;

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of retry attempts for Lago forwarding.
const LAGO_MAX_RETRIES: u32 = 3;

/// Initial backoff duration for retries (doubles with each attempt).
const LAGO_INITIAL_BACKOFF_MS: u64 = 100;

/// Maximum backoff duration for retries.
const LAGO_MAX_BACKOFF_MS: u64 = 5000;

/// Number of API calls that consume 1 credit.
///
/// This defines the conversion rate for API call billing:
/// - 1000 API calls = 1 credit
/// - Minimum charge is always 1 credit
const API_CALLS_PER_CREDIT: u64 = 1000;

/// Usage event request from services.
#[derive(Debug, Deserialize)]
pub struct UsageRequest {
    /// Unique event ID for idempotency.
    pub event_id: String,
    /// User ID being charged.
    pub user_id: String,
    /// Agent ID that generated the usage (optional).
    pub agent_id: Option<String>,
    /// Usage metric details.
    pub metric: UsageMetricRequest,
    /// Pre-calculated cost in cents (optional, will be calculated if not provided).
    pub cost_cents: Option<i64>,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Usage metric in request format.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UsageMetricRequest {
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

/// Usage response.
#[derive(Debug, Serialize)]
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

/// Report a single usage event.
pub async fn report_usage(
    State(state): State<Arc<AppState>>,
    auth: ServiceAuth,
    Json(body): Json<UsageRequest>,
) -> Result<Json<UsageResponse>, ApiError> {
    tracing::debug!(
        service = %auth.service_name,
        event_id = %body.event_id,
        user_id = %body.user_id,
        "Processing usage event"
    );

    let user_id = body
        .user_id
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid user ID".into()))?;

    // Clone agent_id string before consuming it for parsing
    let agent_id_str = body.agent_id.clone();
    let agent_id = agent_id_str
        .as_ref()
        .map(|id| id.parse::<AgentId>())
        .transpose()
        .map_err(|_| ApiError::BadRequest("Invalid agent ID".into()))?;

    // Calculate cost if not provided
    let cost_cents = body
        .cost_cents
        .unwrap_or_else(|| calculate_cost(&state.config.pricing, &body.metric));

    // Build usage event
    let (metric, quantity) = convert_metric(&body.metric);
    let event = UsageEvent {
        event_id: body.event_id.clone(),
        user_id,
        agent_id,
        source: UsageSource::Custom(auth.service_name.clone()),
        metric,
        quantity,
        cost_cents,
        timestamp: chrono::Utc::now(),
        metadata: body.metadata.clone(),
    };

    // Get current balance for transaction record
    let account = state
        .store
        .get_account(&user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    let new_balance = account.balance_cents - cost_cents;

    // Create transaction
    let description = format_usage_description(&body.metric, &auth.service_name);
    let tx = CreditTransaction::usage(user_id, cost_cents, new_balance, description, body.metadata);

    // Process usage atomically
    let balance = state.store.process_usage(&event, &tx)?;

    tracing::info!(
        service = %auth.service_name,
        event_id = %body.event_id,
        user_id = %user_id,
        cost_cents = %cost_cents,
        new_balance = %balance,
        "Usage processed"
    );

    // Check for auto-refill trigger (async, non-blocking)
    maybe_trigger_auto_refill(&state, &account, user_id, balance);

    // Forward to Lago for analytics (async, non-blocking, with retries)
    maybe_forward_to_lago(&state, &body.event_id, user_id, agent_id_str.as_deref(), &body.metric);

    Ok(Json(UsageResponse {
        success: true,
        balance_cents: balance,
        cost_cents,
        transaction_id: tx.id.to_string(),
    }))
}

/// Batch usage request.
#[derive(Debug, Deserialize)]
pub struct BatchUsageRequest {
    /// List of usage events.
    pub events: Vec<UsageRequest>,
}

/// Batch usage response.
#[derive(Debug, Serialize)]
pub struct BatchUsageResponse {
    /// Results for each event.
    pub results: Vec<BatchUsageResult>,
    /// Total events processed.
    pub processed: usize,
    /// Total events failed.
    pub failed: usize,
}

/// Result for a single event in batch.
#[derive(Debug, Serialize)]
pub struct BatchUsageResult {
    /// Event ID.
    pub event_id: String,
    /// Whether successful.
    pub success: bool,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Cost deducted (if successful).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_cents: Option<i64>,
}

/// Report multiple usage events.
pub async fn report_usage_batch(
    State(state): State<Arc<AppState>>,
    auth: ServiceAuth,
    Json(body): Json<BatchUsageRequest>,
) -> Result<Json<BatchUsageResponse>, ApiError> {
    let mut results = Vec::with_capacity(body.events.len());
    let mut processed = 0;
    let mut failed = 0;

    for event_req in body.events {
        let event_id = event_req.event_id.clone();

        // Process each event
        match process_single_usage(&state, &auth.service_name, event_req).await {
            Ok(cost_cents) => {
                results.push(BatchUsageResult {
                    event_id,
                    success: true,
                    error: None,
                    cost_cents: Some(cost_cents),
                });
                processed += 1;
            }
            Err(e) => {
                results.push(BatchUsageResult {
                    event_id,
                    success: false,
                    error: Some(e.to_string()),
                    cost_cents: None,
                });
                failed += 1;
            }
        }
    }

    Ok(Json(BatchUsageResponse {
        results,
        processed,
        failed,
    }))
}

/// Check balance request.
#[derive(Debug, Deserialize)]
pub struct CheckBalanceRequest {
    /// User ID to check.
    pub user_id: String,
    /// Required amount in cents.
    pub required_cents: i64,
}

/// Check balance response.
#[derive(Debug, Serialize)]
pub struct CheckBalanceResponse {
    /// Whether the user has sufficient balance.
    pub sufficient: bool,
    /// Current balance.
    pub balance_cents: i64,
    /// Required amount.
    pub required_cents: i64,
}

/// Check if a user has sufficient balance.
pub async fn check_balance(
    State(state): State<Arc<AppState>>,
    _auth: ServiceAuth,
    Json(body): Json<CheckBalanceRequest>,
) -> Result<Json<CheckBalanceResponse>, ApiError> {
    let user_id = body
        .user_id
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid user ID".into()))?;

    let account = state
        .store
        .get_account(&user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    Ok(Json(CheckBalanceResponse {
        sufficient: account.balance_cents >= body.required_cents,
        balance_cents: account.balance_cents,
        required_cents: body.required_cents,
    }))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if auto-refill should be triggered and spawn the task if needed.
fn maybe_trigger_auto_refill(
    state: &AppState,
    account: &z_billing_core::Account,
    user_id: UserId,
    balance: i64,
) {
    let Some(auto_refill) = &account.auto_refill else {
        return;
    };

    if !auto_refill.enabled || balance >= auto_refill.trigger_below_cents {
        return;
    }

    let stripe = state.stripe.clone();
    let store = state.store.clone();
    let refill_amount = auto_refill.refill_amount_cents;
    let customer_id = account.stripe_customer_id.clone();

    tokio::spawn(async move {
        if let Err(e) = trigger_auto_refill(
            stripe.as_deref(),
            store.as_ref(),
            user_id,
            customer_id,
            refill_amount,
        )
        .await
        {
            tracing::warn!(
                user_id = %user_id,
                error = %e,
                "Failed to trigger auto-refill"
            );
        }
    });
}

/// Forward usage to Lago if configured (with retries).
fn maybe_forward_to_lago(
    state: &AppState,
    event_id: &str,
    user_id: UserId,
    agent_id: Option<&str>,
    metric: &UsageMetricRequest,
) {
    let Some(lago) = &state.lago else {
        return;
    };

    let lago = lago.clone();
    let event_id = event_id.to_string();
    let user_id_str = user_id.to_string();
    let agent_id = agent_id.map(String::from);
    let metric = metric.clone();

    tokio::spawn(async move {
        if let Err(e) = forward_to_lago_with_retry(
            &lago,
            &event_id,
            &user_id_str,
            agent_id.as_deref(),
            &metric,
        )
        .await
        {
            tracing::error!(
                event_id = %event_id,
                error = %e,
                "Failed to forward usage to Lago after all retries"
            );
        }
    });
}

async fn process_single_usage(
    state: &AppState,
    service_name: &str,
    body: UsageRequest,
) -> Result<i64, ApiError> {
    let user_id = body
        .user_id
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid user ID".into()))?;

    let agent_id = body
        .agent_id
        .map(|id| id.parse::<AgentId>())
        .transpose()
        .map_err(|_| ApiError::BadRequest("Invalid agent ID".into()))?;

    let cost_cents = body
        .cost_cents
        .unwrap_or_else(|| calculate_cost(&state.config.pricing, &body.metric));

    let (metric, quantity) = convert_metric(&body.metric);
    let event = UsageEvent {
        event_id: body.event_id.clone(),
        user_id,
        agent_id,
        source: UsageSource::Custom(service_name.to_string()),
        metric,
        quantity,
        cost_cents,
        timestamp: chrono::Utc::now(),
        metadata: body.metadata.clone(),
    };

    let account = state
        .store
        .get_account(&user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    let new_balance = account.balance_cents - cost_cents;
    let description = format_usage_description(&body.metric, service_name);
    let tx = CreditTransaction::usage(user_id, cost_cents, new_balance, description, body.metadata);

    state.store.process_usage(&event, &tx)?;

    Ok(cost_cents)
}

fn calculate_cost(pricing: &z_billing_core::PricingConfig, metric: &UsageMetricRequest) -> i64 {
    match metric {
        UsageMetricRequest::LlmTokens {
            provider,
            model,
            input_tokens,
            output_tokens,
        } => pricing.calculate_llm_cost(provider, model, *input_tokens, *output_tokens),
        UsageMetricRequest::Compute {
            cpu_hours,
            memory_gb_hours,
        } => pricing.calculate_compute_cost(*cpu_hours, *memory_gb_hours),
        UsageMetricRequest::ApiCalls { count, .. } => {
            // Convert API calls to credits using the configured rate, minimum 1 credit
            #[allow(clippy::cast_possible_wrap)]
            std::cmp::max(1, (*count as i64) / (API_CALLS_PER_CREDIT as i64))
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn convert_metric(req: &UsageMetricRequest) -> (UsageMetric, f64) {
    match req {
        UsageMetricRequest::LlmTokens {
            provider,
            model,
            input_tokens,
            output_tokens,
        } => {
            let llm_provider = match provider.to_lowercase().as_str() {
                "anthropic" => LlmProvider::Anthropic,
                "openai" => LlmProvider::OpenAi,
                "google" => LlmProvider::Google,
                other => LlmProvider::Custom(other.to_string()),
            };

            // For simplicity, we'll create a combined metric
            // In practice, you might want separate events for input/output
            let total_tokens = input_tokens + output_tokens;
            (
                UsageMetric::LlmTokens {
                    provider: llm_provider,
                    model: model.clone(),
                    direction: if *output_tokens > *input_tokens {
                        TokenDirection::Output
                    } else {
                        TokenDirection::Input
                    },
                },
                total_tokens as f64,
            )
        }
        UsageMetricRequest::Compute {
            cpu_hours,
            memory_gb_hours,
        } => (
            UsageMetric::Compute {
                cpu_hours: *cpu_hours,
                memory_gb_hours: *memory_gb_hours,
            },
            *cpu_hours,
        ),
        UsageMetricRequest::ApiCalls { endpoint, count } => (
            UsageMetric::ApiCalls {
                endpoint: endpoint.clone(),
            },
            *count as f64,
        ),
    }
}

fn format_usage_description(metric: &UsageMetricRequest, service: &str) -> String {
    match metric {
        UsageMetricRequest::LlmTokens {
            provider,
            model,
            input_tokens,
            output_tokens,
        } => {
            format!(
                "LLM usage: {provider} {model} ({input_tokens} input, {output_tokens} output tokens) via {service}"
            )
        }
        UsageMetricRequest::Compute {
            cpu_hours,
            memory_gb_hours,
        } => {
            format!(
                "Compute usage: {cpu_hours:.2} CPU-hours, {memory_gb_hours:.2} GB-hours via {service}"
            )
        }
        UsageMetricRequest::ApiCalls { endpoint, count } => {
            format!("API calls: {count} calls to {endpoint} via {service}")
        }
    }
}

/// Forward usage event to Lago for analytics with exponential backoff retry.
async fn forward_to_lago_with_retry(
    lago: &crate::lago::LagoClient,
    event_id: &str,
    user_id: &str,
    agent_id: Option<&str>,
    metric: &UsageMetricRequest,
) -> Result<(), crate::lago::client::LagoError> {
    let mut attempt = 0;
    let mut backoff_ms = LAGO_INITIAL_BACKOFF_MS;

    loop {
        match forward_to_lago(lago, event_id, user_id, agent_id, metric).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                attempt += 1;
                
                if attempt >= LAGO_MAX_RETRIES {
                    tracing::warn!(
                        event_id = %event_id,
                        attempt = %attempt,
                        error = %e,
                        "Lago forwarding failed after max retries"
                    );
                    return Err(e);
                }

                tracing::debug!(
                    event_id = %event_id,
                    attempt = %attempt,
                    backoff_ms = %backoff_ms,
                    error = %e,
                    "Lago forwarding failed, retrying"
                );

                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                
                // Exponential backoff with cap
                backoff_ms = (backoff_ms * 2).min(LAGO_MAX_BACKOFF_MS);
            }
        }
    }
}

/// Forward usage event to Lago for analytics.
async fn forward_to_lago(
    lago: &crate::lago::LagoClient,
    event_id: &str,
    user_id: &str,
    agent_id: Option<&str>,
    metric: &UsageMetricRequest,
) -> Result<(), crate::lago::client::LagoError> {
    match metric {
        UsageMetricRequest::LlmTokens {
            provider,
            model,
            input_tokens,
            output_tokens,
        } => {
            lago.send_llm_usage(
                event_id,
                user_id,
                provider,
                model,
                agent_id,
                *input_tokens,
                *output_tokens,
            )
            .await
        }
        UsageMetricRequest::Compute {
            cpu_hours,
            memory_gb_hours,
        } => {
            lago.send_compute_usage(event_id, user_id, agent_id, *cpu_hours, *memory_gb_hours)
                .await
        }
        UsageMetricRequest::ApiCalls { endpoint, count } => {
            // For API calls, we don't currently have a Lago metric defined
            // Could add one if needed
            tracing::debug!(
                event_id = %event_id,
                endpoint = %endpoint,
                count = %count,
                "Skipping Lago forwarding for API calls metric"
            );
            Ok(())
        }
    }
}

/// Trigger an auto-refill purchase via Stripe.
///
/// This function is called when a user's balance drops below their configured
/// auto-refill threshold. It initiates a Stripe payment for the configured
/// refill amount.
async fn trigger_auto_refill(
    stripe: Option<&StripeClient>,
    store: &RocksStore,
    user_id: UserId,
    customer_id: Option<String>,
    amount_cents: i64,
) -> Result<(), String> {
    let stripe = stripe.ok_or("Stripe not configured")?;
    let customer_id = customer_id.ok_or("No Stripe customer ID linked to account")?;

    tracing::info!(
        user_id = %user_id,
        customer_id = %customer_id,
        amount_cents = %amount_cents,
        "Triggering auto-refill"
    );

    // Create a payment intent for the auto-refill amount
    // The customer must have a default payment method set up
    let payment = stripe
        .create_auto_refill_payment(&customer_id, amount_cents)
        .await
        .map_err(|e| format!("Failed to create payment: {e}"))?;

    // If payment requires action (e.g., 3DS), we can't auto-complete it
    if payment.status != "succeeded" {
        tracing::warn!(
            user_id = %user_id,
            payment_id = %payment.id,
            status = %payment.status,
            "Auto-refill payment requires additional action"
        );
        return Err(format!(
            "Payment requires additional action (status: {})",
            payment.status
        ));
    }

    // Payment succeeded, add credits to the account
    let account = store
        .get_account(&user_id)
        .map_err(|e| format!("Failed to get account: {e}"))?
        .ok_or("Account not found")?;

    let new_balance = account.balance_cents + amount_cents;
    let tx = CreditTransaction::auto_refill(user_id, amount_cents, new_balance);

    let balance = store
        .add_credits(&user_id, amount_cents, &tx)
        .map_err(|e| format!("Failed to add credits: {e}"))?;

    tracing::info!(
        user_id = %user_id,
        amount_cents = %amount_cents,
        new_balance = %balance,
        transaction_id = %tx.id,
        "Auto-refill completed successfully"
    );

    Ok(())
}
