//! Webhook handlers for Stripe and Lago.

use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};

use z_billing_core::{CreditTransaction, Plan};
use z_billing_store::Store;

use crate::crypto::{constant_time_eq, hmac_sha256_hex};
use crate::error::ApiError;
use crate::state::AppState;

/// Stripe webhook payload (simplified).
#[derive(Debug, Deserialize)]
pub struct StripeWebhook {
    /// Event type.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Event ID.
    pub id: String,
    /// Event data.
    pub data: StripeEventData,
}

/// Stripe event data container.
#[derive(Debug, Deserialize)]
pub struct StripeEventData {
    /// Event object.
    pub object: serde_json::Value,
}

/// Webhook response.
#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    /// Whether the webhook was processed.
    pub received: bool,
}

/// Handle Stripe webhooks.
pub async fn stripe_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<WebhookResponse>, ApiError> {
    // Get signature header (required even if we skip verification)
    let signature = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok());

    // Verify signature if webhook_secret is configured
    if let Some(webhook_secret) = &state.config.stripe_webhook_secret {
        let sig = signature
            .ok_or_else(|| ApiError::BadRequest("Missing Stripe signature".into()))?;

        if let Some(stripe) = &state.stripe {
            stripe
                .verify_webhook_signature(&body, sig)
                .map_err(|e| {
                    tracing::warn!(error = %e, "Invalid Stripe webhook signature");
                    ApiError::BadRequest("Invalid webhook signature".into())
                })?;
        } else {
            tracing::warn!(
                "Stripe webhook_secret configured but client not available - skipping verification"
            );
        }
        let _ = webhook_secret; // Silence unused warning
    } else {
        // No webhook_secret configured - skip verification (development mode)
        tracing::warn!("Stripe webhook_secret not configured - skipping signature verification");
    }

    // Parse webhook payload
    let webhook: StripeWebhook =
        serde_json::from_str(&body).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    tracing::info!(
        event_type = %webhook.event_type,
        event_id = %webhook.id,
        "Received Stripe webhook"
    );

    // Handle different event types
    match webhook.event_type.as_str() {
        "checkout.session.completed" => {
            handle_checkout_completed(&state, &webhook.data.object).await?;
        }
        "payment_intent.succeeded" => {
            handle_payment_succeeded(&state, &webhook.data.object).await?;
        }
        "customer.subscription.created" | "customer.subscription.updated" => {
            handle_subscription_update(&state, &webhook.data.object).await?;
        }
        "customer.subscription.deleted" => {
            handle_subscription_deleted(&state, &webhook.data.object).await?;
        }
        "invoice.payment_failed" => {
            handle_payment_failed(&state, &webhook.data.object).await?;
        }
        _ => {
            tracing::debug!(event_type = %webhook.event_type, "Unhandled Stripe event");
        }
    }

    Ok(Json(WebhookResponse { received: true }))
}

/// Lago webhook payload.
#[derive(Debug, Deserialize)]
pub struct LagoWebhook {
    /// Webhook type.
    pub webhook_type: String,
    /// Object type.
    pub object_type: String,
    /// Event data.
    #[serde(flatten)]
    pub data: serde_json::Value,
}

/// Handle Lago webhooks.
pub async fn lago_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<WebhookResponse>, ApiError> {
    // Verify webhook signature if secret is configured
    if let Some(webhook_secret) = &state.config.lago_webhook_secret {
        let signature = headers
            .get("x-lago-signature")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::BadRequest("Missing Lago signature".into()))?;

        verify_lago_signature(&body, signature, webhook_secret).map_err(|e| {
            tracing::warn!(error = %e, "Invalid Lago webhook signature");
            ApiError::BadRequest("Invalid webhook signature".into())
        })?;
    } else {
        tracing::warn!("Lago webhook_secret not configured - skipping signature verification");
    }

    // Parse webhook payload
    let webhook: LagoWebhook =
        serde_json::from_str(&body).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    tracing::info!(
        webhook_type = %webhook.webhook_type,
        object_type = %webhook.object_type,
        "Received Lago webhook"
    );

    // Handle different webhook types
    match webhook.webhook_type.as_str() {
        "subscription.started" => {
            handle_lago_subscription_started(&state, &webhook.data).await?;
        }
        "subscription.terminated" => {
            handle_lago_subscription_terminated(&state, &webhook.data).await?;
        }
        "invoice.created" => {
            handle_lago_invoice_created(&state, &webhook.data).await?;
        }
        "subscription.usage_threshold_reached" => {
            handle_lago_usage_threshold(&state, &webhook.data).await?;
        }
        _ => {
            tracing::debug!(webhook_type = %webhook.webhook_type, "Unhandled Lago event");
        }
    }

    Ok(Json(WebhookResponse { received: true }))
}

// Stripe webhook handlers

#[allow(clippy::cast_precision_loss)]
async fn handle_checkout_completed(
    state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    // Extract relevant fields
    let user_id_str = data
        .get("client_reference_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing client_reference_id".into()))?;

    let session_id = data
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let payment_status = data
        .get("payment_status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Only process if payment is complete
    if payment_status != "paid" {
        tracing::info!(
            session_id = %session_id,
            payment_status = %payment_status,
            "Checkout session not paid yet, skipping"
        );
        return Ok(());
    }

    // Get credits amount from metadata
    let credits_amount = data
        .get("metadata")
        .and_then(|m| m.get("credits_amount"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| {
            // Fall back to amount_total if metadata not present
            data.get("amount_total").and_then(serde_json::Value::as_i64)
        })
        .unwrap_or(0);

    let amount_total = data
        .get("amount_total")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);

    let payment_intent = data
        .get("payment_intent")
        .and_then(|v| v.as_str())
        .map(String::from);

    tracing::info!(
        user_id = %user_id_str,
        session_id = %session_id,
        credits_amount = %credits_amount,
        amount_total = %amount_total,
        payment_intent = ?payment_intent,
        "Processing checkout completion"
    );

    // Parse user_id
    let user_id = user_id_str
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("Invalid user_id: {user_id_str}")))?;

    // Get account
    let account = state
        .store
        .get_account(&user_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Account not found for user {user_id_str}")))?;

    // Create transaction
    let new_balance = account.balance_cents + credits_amount;
    let tx = CreditTransaction::purchase(
        user_id,
        credits_amount,
        new_balance,
        format!(
            "Purchased ${:.2} credits via Stripe (session: {})",
            amount_total as f64 / 100.0,
            session_id
        ),
    );

    // Add credits
    let balance = state.store.add_credits(&user_id, credits_amount, &tx)?;

    tracing::info!(
        user_id = %user_id_str,
        credits_added = %credits_amount,
        new_balance = %balance,
        transaction_id = %tx.id,
        "Credits added from Stripe checkout"
    );

    Ok(())
}

async fn handle_payment_succeeded(
    _state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    let payment_intent_id = data.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");

    tracing::info!(
        payment_intent_id = %payment_intent_id,
        "Payment succeeded"
    );

    Ok(())
}

/// Handle Stripe subscription updates.
///
/// # Current Behavior
///
/// This handler currently only logs the event. Actual subscription state is
/// managed through Lago, which is the source of truth for subscription status.
/// Stripe subscription events are logged for audit purposes and potential
/// future reconciliation.
///
/// # Future Considerations
///
/// If direct Stripe subscription management is needed (bypassing Lago), this
/// handler should be extended to:
/// - Update the account's subscription tier in the local store
/// - Sync subscription status with Lago if needed
/// - Handle downgrades/upgrades by adjusting credit grants
async fn handle_subscription_update(
    _state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    let subscription_id = data.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let status = data
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // TODO: Update account subscription status if we move away from Lago
    // for subscription management. Currently Lago is the source of truth.
    tracing::info!(
        subscription_id = %subscription_id,
        status = %status,
        "Subscription updated (informational only - Lago is source of truth)"
    );

    Ok(())
}

/// Handle Stripe subscription deletion.
///
/// # Current Behavior
///
/// This handler currently only logs the event. Subscription cancellation is
/// handled through Lago's `subscription.terminated` webhook, which triggers
/// the actual state update and any necessary cleanup.
///
/// # Future Considerations
///
/// If direct Stripe subscription management is needed, this handler should:
/// - Mark the account's subscription as cancelled
/// - Optionally revoke any remaining subscription credits
/// - Trigger any cancellation workflows (email notifications, etc.)
async fn handle_subscription_deleted(
    _state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    let subscription_id = data.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");

    // TODO: Handle subscription cancellation if we move away from Lago.
    // Currently Lago's subscription.terminated webhook handles this.
    tracing::info!(
        subscription_id = %subscription_id,
        "Subscription deleted (informational only - Lago handles cancellation)"
    );

    Ok(())
}

/// Handle Stripe invoice payment failure.
///
/// # Current Behavior
///
/// This handler logs the payment failure for monitoring and alerting purposes.
/// Stripe handles retry logic automatically based on the account's retry settings.
///
/// # Future Considerations
///
/// For production hardening, consider:
/// - Sending notification emails to affected users
/// - Updating account status to `payment_failed` for UI indication
/// - Pausing service access after N consecutive failures
/// - Integrating with alerting systems (`PagerDuty`, Slack, etc.)
async fn handle_payment_failed(
    _state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    let invoice_id = data.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");

    // TODO: Implement notification system for payment failures.
    // Consider: email notifications, account status updates, service pausing.
    tracing::warn!(
        invoice_id = %invoice_id,
        "Payment failed - user may need to update payment method"
    );

    Ok(())
}

// Lago webhook handlers

async fn handle_lago_subscription_started(
    state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    let subscription = data.get("subscription");
    
    let external_customer_id = subscription
        .and_then(|s| s.get("external_customer_id"))
        .and_then(|v| v.as_str());

    let plan_code = subscription
        .and_then(|s| s.get("plan_code"))
        .and_then(|v| v.as_str());

    let lago_subscription_id = subscription
        .and_then(|s| s.get("lago_id"))
        .and_then(|v| v.as_str());

    tracing::info!(
        external_customer_id = ?external_customer_id,
        plan_code = ?plan_code,
        lago_subscription_id = ?lago_subscription_id,
        "Lago subscription started"
    );

    // Parse user_id from external_customer_id
    let user_id_str = external_customer_id.ok_or_else(|| {
        ApiError::BadRequest("Missing external_customer_id in subscription".into())
    })?;

    let user_id = user_id_str.parse().map_err(|_| {
        ApiError::BadRequest(format!("Invalid user_id: {user_id_str}"))
    })?;

    // Determine plan from plan_code
    let plan = match plan_code {
        Some("standard" | "plan_standard") => Plan::Standard,
        Some("pro" | "plan_pro") => Plan::Pro,
        Some(code) => {
            tracing::warn!(plan_code = %code, "Unknown plan code, treating as Free");
            Plan::Free
        }
        None => Plan::Free,
    };

    // Get monthly credits for this plan
    let monthly_credits = plan.monthly_credits();
    
    if monthly_credits == 0 {
        tracing::debug!(
            user_id = %user_id_str,
            plan_code = ?plan_code,
            "Plan has no monthly credits to grant"
        );
        return Ok(());
    }

    // Get or create account
    let account = if let Some(acc) = state.store.get_account(&user_id)? {
        acc
    } else {
        tracing::warn!(
            user_id = %user_id_str,
            "Account not found for Lago subscription, creating new account"
        );
        let new_account = z_billing_core::Account::new(user_id);
        state.store.put_account(&new_account)?;
        new_account
    };

    // Create transaction for subscription credits
    let new_balance = account.balance_cents + monthly_credits;
    let plan_name = format!("{plan:?}");
    let tx = CreditTransaction::subscription_grant(
        user_id,
        monthly_credits,
        new_balance,
        &plan_name,
    );

    // Add credits
    let balance = state.store.add_credits(&user_id, monthly_credits, &tx)?;

    tracing::info!(
        user_id = %user_id_str,
        plan = ?plan,
        credits_granted = %monthly_credits,
        new_balance = %balance,
        transaction_id = %tx.id,
        "Monthly subscription credits granted"
    );

    Ok(())
}

/// Handle Lago subscription termination.
///
/// # Current Behavior
///
/// This handler logs the subscription termination. Since credits are granted
/// at subscription start (via `handle_lago_subscription_started`), termination
/// does not automatically revoke credits - users keep their remaining balance.
///
/// # Design Decision
///
/// Credits are not revoked on cancellation because:
/// 1. Users may have purchased additional credits beyond their subscription grant
/// 2. It provides a better user experience (no surprise credit loss)
/// 3. Remaining credits will naturally deplete through usage
///
/// If credit revocation is needed, implement logic to:
/// - Calculate and revoke only the pro-rated unused subscription credits
/// - Preserve any purchased credits
async fn handle_lago_subscription_terminated(
    _state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    let external_customer_id = data
        .get("subscription")
        .and_then(|s| s.get("external_customer_id"))
        .and_then(|v| v.as_str());

    // NOTE: We intentionally do not revoke credits on termination.
    // Users keep their remaining balance until depleted through usage.
    tracing::info!(
        external_customer_id = ?external_customer_id,
        "Lago subscription terminated (credits retained)"
    );

    Ok(())
}

async fn handle_lago_invoice_created(
    _state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    let invoice_id = data
        .get("invoice")
        .and_then(|i| i.get("lago_id"))
        .and_then(|v| v.as_str());

    tracing::info!(
        invoice_id = ?invoice_id,
        "Lago invoice created"
    );

    Ok(())
}

async fn handle_lago_usage_threshold(
    _state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    let subscription_id = data
        .get("subscription")
        .and_then(|s| s.get("lago_id"))
        .and_then(|v| v.as_str());

    tracing::info!(
        subscription_id = ?subscription_id,
        "Lago usage threshold reached"
    );

    Ok(())
}

// ============================================================================
// Signature Verification Helpers
// ============================================================================

/// Verify Lago webhook signature using HMAC-SHA256.
///
/// Lago signs webhooks with HMAC-SHA256 using the webhook secret.
/// The signature header contains the hex-encoded signature.
fn verify_lago_signature(body: &str, signature: &str, secret: &str) -> Result<(), String> {
    let expected = hmac_sha256_hex(secret, body);

    // Use constant-time comparison to prevent timing attacks
    if constant_time_eq(&expected, signature) {
        Ok(())
    } else {
        Err("Signature mismatch".into())
    }
}
