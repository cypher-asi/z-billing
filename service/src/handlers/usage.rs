//! Usage event handlers.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use z_billing_core::{
    AgentId, CreditTransaction, LlmProvider, TokenDirection, UsageEvent, UsageMetric, UsageSource,
};
use z_billing_store::Store;

use crate::auth::ServiceAuth;
use crate::error::ApiError;
use crate::state::AppState;

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

    // Forward to Lago for analytics (async, non-blocking)
    if let Some(lago) = &state.lago {
        let lago = lago.clone();
        let event_id = body.event_id.clone();
        let user_id_str = user_id.to_string();
        let lago_agent_id = agent_id_str.clone();
        let metric = body.metric.clone();

        tokio::spawn(async move {
            if let Err(e) = forward_to_lago(
                &lago,
                &event_id,
                &user_id_str,
                lago_agent_id.as_deref(),
                &metric,
            )
            .await
            {
                tracing::warn!(
                    event_id = %event_id,
                    error = %e,
                    "Failed to forward usage to Lago"
                );
            }
        });
    }

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

// Helper functions

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
            // 1 credit per 1000 API calls, minimum 1
            #[allow(clippy::cast_possible_wrap)]
            std::cmp::max(1, (*count as i64) / 1000)
        }
    }
}

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
