//! Webhook handlers for Stripe and Lago.

use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};

use z_billing_core::{CreditTransaction, Plan, Subscription, SubscriptionStatus};
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

    // Verify signature -- reject if webhook secret or client is not configured
    let stripe = state.stripe.as_ref().ok_or_else(|| {
        tracing::error!("Stripe webhook received but Stripe client not configured");
        ApiError::Internal("Webhook processing unavailable".into())
    })?;

    if state.config.stripe_webhook_secret.is_none() {
        tracing::error!("Stripe webhook received but STRIPE_WEBHOOK_SECRET not configured");
        return Err(ApiError::Internal(
            "Webhook processing unavailable".into(),
        ));
    }

    let sig =
        signature.ok_or_else(|| ApiError::BadRequest("Missing Stripe signature".into()))?;

    stripe.verify_webhook_signature(&body, sig).map_err(|e| {
        tracing::warn!(error = %e, "Invalid Stripe webhook signature");
        ApiError::BadRequest("Invalid webhook signature".into())
    })?;

    // Parse webhook payload
    let webhook: StripeWebhook =
        serde_json::from_str(&body).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    tracing::info!(
        event_type = %webhook.event_type,
        event_id = %webhook.id,
        "Received Stripe webhook"
    );

    // Replay protection: reject if this event was already processed
    if state.store.has_webhook_event(&webhook.id)? {
        tracing::warn!(event_id = %webhook.id, "Duplicate Stripe webhook event, skipping");
        return Ok(Json(WebhookResponse { received: true }));
    }

    // Handle different event types
    let event_type = webhook.event_type.as_str();
    match event_type {
        "checkout.session.completed" => {
            handle_checkout_completed(&state, &webhook.data.object).await?;
        }
        "payment_intent.succeeded" => {
            handle_payment_succeeded(&state, &webhook.data.object).await?;
        }
        "customer.subscription.created" | "customer.subscription.updated" => {
            handle_subscription_update(&state, &webhook.data.object, event_type).await?;
        }
        "customer.subscription.deleted" => {
            handle_subscription_deleted(&state, &webhook.data.object).await?;
        }
        "invoice.paid" => {
            handle_invoice_paid(&state, &webhook.data.object).await?;
        }
        "invoice.payment_failed" => {
            handle_payment_failed(&state, &webhook.data.object).await?;
        }
        _ => {
            tracing::debug!(event_type = %webhook.event_type, "Unhandled Stripe event");
        }
    }

    // Record event as processed for replay protection
    state.store.record_webhook_event(&webhook.id, "stripe")?;

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
    // Verify webhook signature -- reject if secret is not configured
    let webhook_secret = state.config.lago_webhook_secret.as_ref().ok_or_else(|| {
        tracing::error!("Lago webhook received but LAGO_WEBHOOK_SECRET not configured");
        ApiError::Internal("Webhook processing unavailable".into())
    })?;

    let signature = headers
        .get("x-lago-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::BadRequest("Missing Lago signature".into()))?;

    verify_lago_signature(&body, signature, webhook_secret).map_err(|e| {
        tracing::warn!(error = %e, "Invalid Lago webhook signature");
        ApiError::BadRequest("Invalid webhook signature".into())
    })?;

    // Parse webhook payload
    let webhook: LagoWebhook =
        serde_json::from_str(&body).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    tracing::info!(
        webhook_type = %webhook.webhook_type,
        object_type = %webhook.object_type,
        "Received Lago webhook"
    );

    // Build dedup key from webhook_type + inner lago_id (Lago has no top-level event ID)
    let lago_id = webhook
        .data
        .get(&webhook.object_type)
        .or_else(|| webhook.data.get("subscription"))
        .or_else(|| webhook.data.get("invoice"))
        .and_then(|obj| obj.get("lago_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let dedup_key = format!("lago:{}:{}", webhook.webhook_type, lago_id);

    // Replay protection: reject if this event was already processed
    if state.store.has_webhook_event(&dedup_key)? {
        tracing::warn!(dedup_key = %dedup_key, "Duplicate Lago webhook event, skipping");
        return Ok(Json(WebhookResponse { received: true }));
    }

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

    // Record event as processed for replay protection
    state.store.record_webhook_event(&dedup_key, "lago")?;

    Ok(Json(WebhookResponse { received: true }))
}

// Stripe webhook handlers

#[allow(clippy::cast_precision_loss)]
async fn handle_checkout_completed(
    state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    let mode = data.get("mode").and_then(|v| v.as_str()).unwrap_or("payment");

    // Subscription checkouts are handled via customer.subscription.created webhook
    // which fires after checkout. Here we just save the stripe_customer_id.
    if mode == "subscription" {
        let user_id_str = data.get("client_reference_id").and_then(|v| v.as_str());
        let customer_id = data.get("customer").and_then(|v| v.as_str());

        if let (Some(uid_str), Some(cid)) = (user_id_str, customer_id) {
            if let Ok(user_id) = uid_str.parse::<z_billing_core::UserId>() {
                let mut account = match state.store.get_account(&user_id)? {
                    Some(a) => a,
                    None => {
                        let a = z_billing_core::Account::new(user_id);
                        state.store.put_account(&a)?;
                        a
                    }
                };
                account.stripe_customer_id = Some(cid.to_string());
                account.updated_at = chrono::Utc::now();
                state.store.put_account(&account)?;

                tracing::info!(
                    user_id = %uid_str,
                    customer_id = %cid,
                    "Subscription checkout completed — customer ID saved"
                );
            }
        }
        return Ok(());
    }

    // Credit purchase checkout (mode=payment)
    // Extract relevant fields
    let user_id_str = data
        .get("client_reference_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing client_reference_id".into()))?;

    let session_id = data.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");

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

    // Broadcast balance update to WebSocket clients
    #[allow(clippy::cast_precision_loss)]
    let _ = state.balance_tx.send(
        serde_json::json!({
            "type": "balance.updated",
            "userId": user_id_str,
            "balanceCents": balance,
            "balanceFormatted": format!("${:.2}", balance as f64 / 100.0),
        })
        .to_string(),
    );

    tracing::info!(
        user_id = %user_id_str,
        credits_added = %credits_amount,
        new_balance = %balance,
        transaction_id = %tx.id,
        "Credits added from Stripe checkout"
    );

    crate::mixpanel::track(
        state.config.mixpanel_token.as_deref(),
        "payment_completed",
        &user_id_str,
        serde_json::json!({
            "amount_cents": amount_total,
            "amount_dollars": amount_total as f64 / 100.0,
            "credits_purchased": credits_amount,
            "balance_after": balance,
        }),
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

/// Resolve a Plan from a Stripe price ID by checking env vars.
fn plan_from_stripe_price_id(price_id: &str) -> Plan {
    let pro_price = std::env::var("STRIPE_PRICE_PRO").unwrap_or_default();
    let pro_legacy = std::env::var("STRIPE_PRICE_PRO_LEGACY").unwrap_or_default();
    let crusader_price = std::env::var("STRIPE_PRICE_CRUSADER").unwrap_or_default();
    let sage_price = std::env::var("STRIPE_PRICE_SAGE").unwrap_or_default();

    if price_id == pro_price || price_id == pro_legacy {
        Plan::Pro
    } else if price_id == crusader_price {
        Plan::Crusader
    } else if price_id == sage_price {
        Plan::Sage
    } else {
        tracing::warn!(price_id = %price_id, "Unknown Stripe price ID, defaulting to Mortal");
        Plan::Mortal
    }
}

/// Extract user ID from a Stripe subscription or invoice object.
///
/// Tries metadata first, then falls back to looking up the account
/// by the Stripe customer ID (saved during checkout.session.completed).
fn extract_user_id(data: &serde_json::Value, state: &AppState) -> Option<z_billing_core::UserId> {
    // Try metadata.user_id on the object itself
    let uid_str = data
        .get("metadata")
        .and_then(|m| m.get("user_id"))
        .and_then(|v| v.as_str())
        // Try subscription_details.metadata (for invoices)
        .or_else(|| {
            data.get("subscription_details")
                .and_then(|d| d.get("metadata"))
                .and_then(|m| m.get("user_id"))
                .and_then(|v| v.as_str())
        });

    if let Some(uid) = uid_str.and_then(|s| s.parse().ok()) {
        return Some(uid);
    }

    // Fallback: look up account by Stripe customer ID
    let customer_id = data.get("customer").and_then(|v| v.as_str())?;
    let account = state.store.find_account_by_stripe_customer(customer_id).ok()??;
    Some(account.user_id)
}

/// Handle Stripe subscription created or updated.
///
/// Updates the z-billing Account to mirror Stripe subscription state.
/// Handles new subscriptions, plan changes, cancellation pending, and status changes.
async fn handle_subscription_update(
    state: &AppState,
    data: &serde_json::Value,
    event_type: &str,
) -> Result<(), ApiError> {
    let subscription_id = data.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let status = data.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
    let cancel_at_period_end = data.get("cancel_at_period_end").and_then(|v| v.as_bool()).unwrap_or(false);

    let user_id = match extract_user_id(data, state) {
        Some(uid) => uid,
        None => {
            tracing::warn!(subscription_id = %subscription_id, "Subscription update — no user_id in metadata or customer lookup, skipping");
            return Ok(());
        }
    };

    // Resolve plan from price ID
    let price_id = data
        .get("items")
        .and_then(|i| i.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|item| item.get("price"))
        .and_then(|p| p.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let plan = plan_from_stripe_price_id(price_id);

    // Parse billing period
    let period_start = data.get("current_period_start").and_then(|v| v.as_i64())
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(chrono::Utc::now);
    let period_end = data.get("current_period_end").and_then(|v| v.as_i64())
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(chrono::Utc::now);

    // Map Stripe status
    let sub_status = if cancel_at_period_end {
        SubscriptionStatus::Cancelled
    } else {
        match status {
            "active" | "trialing" => SubscriptionStatus::Active,
            "past_due" => SubscriptionStatus::PastDue,
            _ => SubscriptionStatus::Cancelled,
        }
    };

    // Update account
    let mut account = match state.store.get_account(&user_id)? {
        Some(a) => a,
        None => {
            let a = z_billing_core::Account::new(user_id);
            state.store.put_account(&a)?;
            a
        }
    };

    if let Some(cid) = data.get("customer").and_then(|v| v.as_str()) {
        account.stripe_customer_id = Some(cid.to_string());
    }

    account.subscription = Some(Subscription {
        plan: plan.clone(),
        status: sub_status,
        current_period_start: period_start,
        current_period_end: period_end,
        lago_subscription_id: String::new(),
        stripe_subscription_id: Some(subscription_id.to_string()),
        created_at: account.subscription.as_ref().map_or_else(chrono::Utc::now, |s| s.created_at),
    });
    account.updated_at = chrono::Utc::now();
    state.store.put_account(&account)?;

    // Grant referral credits on first subscription if this user was referred.
    // Only fires once — checked via ReferralBonus transaction history.
    if let Some(ref inviter_id_str) = account.referred_by {
        if let Ok(inviter_id) = inviter_id_str.parse::<z_billing_core::UserId>() {
            // Check if referral already granted
            let txs = state.store.list_transactions_by_user(&user_id, 100, 0)?;
            let already_granted = txs.iter().any(|t| {
                t.transaction_type == z_billing_core::TransactionType::ReferralBonus
            });

            if !already_granted {
                let amount = super::credits::referral_grant_amount();

                // Grant to invitee
                let invitee_balance = {
                    let acc = state.store.get_account(&user_id)?.unwrap_or(account.clone());
                    let nb = acc.balance_cents + amount;
                    let tx = CreditTransaction::referral_bonus(user_id, amount, nb, format!("Referral bonus — invited by {inviter_id_str}"));
                    state.store.add_credits(&user_id, amount, &tx)?
                };

                // Grant to inviter
                let inviter_balance = {
                    let acc = match state.store.get_account(&inviter_id)? {
                        Some(a) => a,
                        None => {
                            let a = z_billing_core::Account::new(inviter_id);
                            state.store.put_account(&a)?;
                            a
                        }
                    };
                    let nb = acc.balance_cents + amount;
                    let tx = CreditTransaction::referral_bonus(inviter_id, amount, nb, format!("Referral bonus — {} subscribed", user_id));
                    state.store.add_credits(&inviter_id, amount, &tx)?
                };

                tracing::info!(
                    user_id = %user_id,
                    inviter_id = %inviter_id_str,
                    amount = %amount,
                    "Referral credits granted on first subscription"
                );

                // Broadcast balance updates
                #[allow(clippy::cast_precision_loss)]
                let _ = state.balance_tx.send(serde_json::json!({
                    "type": "balance.updated",
                    "userId": user_id.to_string(),
                    "balanceCents": invitee_balance,
                }).to_string());
                let _ = state.balance_tx.send(serde_json::json!({
                    "type": "balance.updated",
                    "userId": inviter_id_str,
                    "balanceCents": inviter_balance,
                }).to_string());
            }
        }
    }

    tracing::info!(
        user_id = %user_id,
        subscription_id = %subscription_id,
        plan = ?plan,
        status = %status,
        cancel_at_period_end = %cancel_at_period_end,
        "Subscription synced to z-billing"
    );

    if event_type == "customer.subscription.created" {
        crate::mixpanel::track(
            state.config.mixpanel_token.as_deref(),
            "subscription_created",
            &user_id.to_string(),
            serde_json::json!({
                "plan": format!("{plan:?}"),
                "subscription_id": subscription_id,
            }),
        );
    }

    // Sync pro status to zos-api (any paid tier = pro).
    // cancel_at_period_end means user still has access until period ends,
    // so they remain pro. Only handle_subscription_deleted revokes pro.
    let is_pro = plan != Plan::Mortal;
    sync_pro_status_to_zos(state, &user_id, is_pro);

    Ok(())
}

/// Handle Stripe subscription deleted (fully ended).
///
/// Reverts the account to Mortal tier.
async fn handle_subscription_deleted(
    state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    let subscription_id = data.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");

    let user_id = match extract_user_id(data, state) {
        Some(uid) => uid,
        None => {
            tracing::warn!(subscription_id = %subscription_id, "Subscription deleted — no user_id, skipping");
            return Ok(());
        }
    };

    if let Some(mut account) = state.store.get_account(&user_id)? {
        account.subscription = None;
        account.updated_at = chrono::Utc::now();
        state.store.put_account(&account)?;

        tracing::info!(user_id = %user_id, subscription_id = %subscription_id, "Subscription ended — reverted to Mortal");

        crate::mixpanel::track(
            state.config.mixpanel_token.as_deref(),
            "subscription_cancelled",
            &user_id.to_string(),
            serde_json::json!({}),
        );

        // Sync pro status to zos-api (no subscription = not pro)
        sync_pro_status_to_zos(state, &user_id, false);
    }

    Ok(())
}

/// Handle invoice.paid — grant monthly credits on subscription renewal.
#[allow(clippy::cast_precision_loss)]
async fn handle_invoice_paid(
    state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    // Only process subscription invoices
    let subscription_id = match data.get("subscription").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return Ok(()),
    };

    let user_id = match extract_user_id(data, state) {
        Some(uid) => uid,
        None => {
            tracing::warn!(subscription_id = %subscription_id, "invoice.paid — no user_id, skipping credit grant");
            return Ok(());
        }
    };

    let account = match state.store.get_account(&user_id)? {
        Some(a) => a,
        None => {
            tracing::warn!(user_id = %user_id, "invoice.paid — account not found");
            return Ok(());
        }
    };

    let plan = account.current_plan();
    let plan_credits = plan.monthly_credits();
    if plan_credits <= 0 {
        return Ok(());
    }

    // Check how many monthly allowance credits have already been granted
    // in the last 30 days to avoid double-granting on plan changes.
    // Uses a 30-day window rather than subscription period_start because
    // the Mortal lazy monthly grant fires before any subscription exists.
    let window_start = account
        .last_monthly_grant_at
        .map(|t| t - chrono::Duration::days(1))
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(30));

    let txs = state.store.list_transactions_by_user(&user_id, 100, 0)?;
    let already_granted: i64 = txs.iter()
        .filter(|t| {
            t.transaction_type == z_billing_core::TransactionType::MonthlyAllowance
                && t.created_at >= window_start
        })
        .map(|t| t.amount_cents)
        .sum();

    let credits = (plan_credits - already_granted).max(0);
    if credits <= 0 {
        tracing::info!(
            user_id = %user_id,
            plan = ?plan,
            plan_credits = %plan_credits,
            already_granted = %already_granted,
            subscription_id = %subscription_id,
            "Monthly credits already granted this period, skipping"
        );
        return Ok(());
    }

    let new_balance = account.balance_cents + credits;
    let tx = CreditTransaction::monthly_allowance(user_id, credits, new_balance);
    let balance = state.store.add_credits(&user_id, credits, &tx)?;

    // Update last_monthly_grant_at
    let mut updated = state.store.get_account(&user_id)?.unwrap_or(account);
    updated.last_monthly_grant_at = Some(chrono::Utc::now());
    updated.updated_at = chrono::Utc::now();
    state.store.put_account(&updated)?;

    let _ = state.balance_tx.send(
        serde_json::json!({
            "type": "balance.updated",
            "userId": user_id.to_string(),
            "balanceCents": balance,
            "balanceFormatted": format!("${:.2}", balance as f64 / 100.0),
        })
        .to_string(),
    );

    tracing::info!(
        user_id = %user_id,
        plan = ?plan,
        credits = %credits,
        already_granted = %already_granted,
        balance = %balance,
        subscription_id = %subscription_id,
        "Monthly credits granted via invoice.paid (prorated)"
    );

    let invoice_amount_cents = data.get("amount_paid").and_then(|v| v.as_i64()).unwrap_or(0);
    crate::mixpanel::track(
        state.config.mixpanel_token.as_deref(),
        "subscription_payment_received",
        &user_id.to_string(),
        serde_json::json!({
            "plan": format!("{plan:?}"),
            "amount_cents": invoice_amount_cents,
            "amount_dollars": invoice_amount_cents as f64 / 100.0,
            "credits_granted": credits,
            "balance_after": balance,
        }),
    );

    Ok(())
}

/// Handle invoice payment failure — mark subscription as past_due.
async fn handle_payment_failed(
    state: &AppState,
    data: &serde_json::Value,
) -> Result<(), ApiError> {
    let invoice_id = data.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");

    if let Some(user_id) = extract_user_id(data, state) {
        if let Some(mut account) = state.store.get_account(&user_id)? {
            if let Some(ref mut sub) = account.subscription {
                sub.status = SubscriptionStatus::PastDue;
                account.updated_at = chrono::Utc::now();
                state.store.put_account(&account)?;

                tracing::warn!(user_id = %user_id, invoice_id = %invoice_id, "Payment failed — subscription past_due");

                crate::mixpanel::track(
                    state.config.mixpanel_token.as_deref(),
                    "payment_failed",
                    &user_id.to_string(),
                    serde_json::json!({}),
                );

                return Ok(());
            }
        }
    }

    tracing::warn!(invoice_id = %invoice_id, "Payment failed — could not resolve user");
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

    let user_id = user_id_str
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("Invalid user_id: {user_id_str}")))?;

    // Determine plan from plan_code
    let plan = match plan_code {
        Some("pro" | "plan_pro") => Plan::Pro,
        Some("crusader" | "plan_crusader") => Plan::Crusader,
        Some("sage" | "plan_sage") => Plan::Sage,
        // Legacy mappings
        Some("standard" | "plan_standard") => Plan::Pro,
        Some(code) => {
            tracing::warn!(plan_code = %code, "Unknown plan code, treating as Mortal");
            Plan::Mortal
        }
        None => Plan::Mortal,
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
    let tx =
        CreditTransaction::subscription_grant(user_id, monthly_credits, new_balance, &plan_name);

    // Add credits
    let balance = state.store.add_credits(&user_id, monthly_credits, &tx)?;

    // Broadcast balance update to WebSocket clients
    #[allow(clippy::cast_precision_loss)]
    let _ = state.balance_tx.send(
        serde_json::json!({
            "type": "balance.updated",
            "userId": user_id_str,
            "balanceCents": balance,
            "balanceFormatted": format!("${:.2}", balance as f64 / 100.0),
        })
        .to_string(),
    );

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

/// Sync subscription pro status to zos-api.
///
/// Calls zos-api's internal billing endpoint to update the user's
/// `isZeroPro` flag. Any paid tier (Pro/Crusader/Sage) sets it to true,
/// Mortal (or no subscription) sets it to false.
///
/// Fire-and-forget: logs errors but does not fail the webhook handler.
fn sync_pro_status_to_zos(state: &AppState, user_id: &z_billing_core::UserId, is_pro: bool) {
    let zos_url = match &state.config.zos_api_url {
        Some(url) => url.clone(),
        None => return, // Not configured, skip silently
    };
    let zos_token = match &state.config.zos_api_internal_token {
        Some(token) => token.clone(),
        None => return,
    };

    let url = format!("{}/internal/billing/pro-status-changed", zos_url);
    let user_id_str = user_id.to_string();

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        match client
            .post(&url)
            .header("x-internal-token", &zos_token)
            .json(&serde_json::json!({
                "userId": user_id_str,
                "isZeroPro": is_pro,
            }))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(
                    user_id = %user_id_str,
                    is_pro = %is_pro,
                    "Synced pro status to zos-api"
                );
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                tracing::warn!(
                    user_id = %user_id_str,
                    status = %status,
                    body = %body,
                    "Failed to sync pro status to zos-api"
                );
            }
            Err(err) => {
                tracing::warn!(
                    user_id = %user_id_str,
                    error = %err,
                    "Failed to sync pro status to zos-api"
                );
            }
        }
    });
}
