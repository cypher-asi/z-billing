//! Subscription management handlers.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use z_billing_core::Plan;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

// ============================================================================
// Types
// ============================================================================

/// Request to create a subscription checkout session.
#[derive(Debug, Deserialize)]
pub struct SubscriptionCheckoutRequest {
    /// The plan to subscribe to.
    pub plan: String,
}

/// Response with a checkout URL.
#[derive(Debug, Serialize)]
pub struct CheckoutResponse {
    /// The Stripe Checkout URL to redirect the user to.
    pub url: String,
}

/// Current subscription status.
#[derive(Debug, Serialize)]
pub struct SubscriptionStatusResponse {
    /// Current plan name.
    pub plan: String,
    /// Whether the user has an active paid subscription.
    pub is_subscribed: bool,
    /// Monthly credit allowance for the current plan.
    pub monthly_credits: i64,
    /// End of current billing period (next renewal date). Null for free tier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_period_end: Option<String>,
}

// ============================================================================
// Price ID resolution
// ============================================================================

fn stripe_price_id_for_plan(plan: &str) -> Result<String, ApiError> {
    let env_key = match plan {
        "pro" => "STRIPE_PRICE_PRO",
        "crusader" => "STRIPE_PRICE_CRUSADER",
        "sage" => "STRIPE_PRICE_SAGE",
        _ => {
            return Err(ApiError::BadRequest(format!(
                "Invalid plan: '{plan}'. Must be pro, crusader, or sage."
            )));
        }
    };

    std::env::var(env_key).map_err(|_| {
        ApiError::Internal(format!(
            "Stripe price not configured for plan '{plan}' (missing {env_key})"
        ))
    })
}

// ============================================================================
// Handlers
// ============================================================================

/// Create a Stripe Checkout session for subscribing to a tier.
///
/// Returns a checkout URL that the frontend should redirect the user to.
pub async fn checkout(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SubscriptionCheckoutRequest>,
) -> Result<Json<CheckoutResponse>, ApiError> {
    let price_id = stripe_price_id_for_plan(&body.plan)?;

    let stripe = state.stripe.as_ref().ok_or_else(|| {
        ApiError::Internal("Stripe not configured".into())
    })?;

    // Check if user already has a Stripe customer ID
    let account = state.store.get_account(&auth.user_id)?;
    let customer_id = account.as_ref().and_then(|a| a.stripe_customer_id.as_deref());

    // Prevent duplicate subscriptions — if user has any subscription (active or
    // cancelling but not yet expired), they should use the Customer Portal instead.
    if let Some(ref acc) = account {
        if acc.subscription.is_some() {
            return Err(ApiError::BadRequest(
                "You already have a subscription. Use the Customer Portal to manage or change plans.".into(),
            ));
        }
    }

    let success_url = format!(
        "{}/checkout/success",
        state.config.frontend_url
    );
    let cancel_url = format!("{}/checkout/cancelled", state.config.frontend_url);

    let session = stripe
        .create_subscription_checkout(
            customer_id,
            &auth.user_id.to_string(),
            &price_id,
            &success_url,
            &cancel_url,
        )
        .await
        .map_err(|e| ApiError::Internal(format!("Stripe checkout failed: {e}")))?;

    let url = session.url.ok_or_else(|| {
        ApiError::Internal("Stripe returned no checkout URL".into())
    })?;

    tracing::info!(
        user_id = %auth.user_id,
        plan = %body.plan,
        "Subscription checkout session created"
    );

    Ok(Json(CheckoutResponse { url }))
}

/// Create a Stripe Customer Portal session for managing an existing subscription.
///
/// Returns a portal URL for card updates, plan changes, and cancellation.
pub async fn portal(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<CheckoutResponse>, ApiError> {
    let stripe = state.stripe.as_ref().ok_or_else(|| {
        ApiError::Internal("Stripe not configured".into())
    })?;

    let account = state
        .store
        .get_account(&auth.user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    let customer_id = account.stripe_customer_id.as_deref().ok_or_else(|| {
        ApiError::BadRequest("No Stripe customer found. Subscribe to a plan first.".into())
    })?;

    let return_url = format!("{}/settings", state.config.frontend_url);

    let portal = stripe
        .create_portal_session(customer_id, &return_url)
        .await
        .map_err(|e| ApiError::Internal(format!("Stripe portal failed: {e}")))?;

    let url = portal
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::Internal("Stripe returned no portal URL".into()))?
        .to_string();

    Ok(Json(CheckoutResponse { url }))
}

/// Get the current subscription status.
pub async fn status(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<SubscriptionStatusResponse>, ApiError> {
    let account = state
        .store
        .get_account(&auth.user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    let plan = account.current_plan();
    let is_subscribed = account.subscription.is_some()
        && account
            .subscription
            .as_ref()
            .map_or(false, |s| s.status == z_billing_core::SubscriptionStatus::Active);

    let period_end = account
        .subscription
        .as_ref()
        .map(|s| s.current_period_end.to_rfc3339());

    Ok(Json(SubscriptionStatusResponse {
        plan: format!("{:?}", plan).to_lowercase(),
        is_subscribed,
        monthly_credits: plan.monthly_credits(),
        current_period_end: period_end,
    }))
}

// ============================================================================
// zOS-compatible subscription endpoints
//
// These endpoints match the API contract expected by the zos frontend
// billing module (/api/subscriptions/zero-pro, /status, /cancel).
// ============================================================================

/// Request body for zos Zero Pro subscription.
#[derive(Debug, Deserialize)]
pub struct ZosSubscribeRequest {
    #[serde(rename = "billingDetails")]
    pub billing_details: ZosBillingDetails,
    #[serde(rename = "paymentMethodId")]
    pub payment_method_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ZosBillingDetails {
    pub email: Option<String>,
    pub name: Option<String>,
    pub address: Option<serde_json::Value>,
}

/// Response for zos subscribe.
#[derive(Debug, Serialize)]
pub struct ZosSubscribeResponse {
    #[serde(rename = "subscriptionId")]
    pub subscription_id: String,
    #[serde(rename = "clientSecret")]
    pub client_secret: Option<String>,
    pub status: String,
}

/// Response for zos status.
#[derive(Debug, Serialize)]
pub struct ZosStatusResponse {
    pub subscription: Option<ZosSubscriptionInfo>,
}

#[derive(Debug, Serialize)]
pub struct ZosSubscriptionInfo {
    pub status: String,
    #[serde(rename = "type")]
    pub sub_type: String,
    #[serde(rename = "stripeSubscriptionId")]
    pub stripe_subscription_id: String,
    #[serde(rename = "currentPeriodEnd")]
    pub current_period_end: Option<String>,
}

/// Response for zos cancel.
#[derive(Debug, Serialize)]
pub struct ZosCancelResponse {
    pub cancelled: bool,
}

/// Create a Zero Pro subscription (zos-compatible inline card flow).
///
/// Accepts a payment method ID from the frontend Stripe Elements card form,
/// creates or retrieves a Stripe customer, creates a subscription on the
/// Pro tier, and returns a client secret for payment confirmation.
pub async fn subscribe_zero_pro(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<ZosSubscribeRequest>,
) -> Result<Json<ZosSubscribeResponse>, ApiError> {
    let stripe = state.stripe.as_ref().ok_or_else(|| {
        ApiError::Internal("Stripe not configured".into())
    })?;

    // Check for existing subscription
    let account = state.store.get_account(&auth.user_id)?;
    if let Some(ref acc) = account {
        if acc.subscription.is_some() {
            return Err(ApiError::BadRequest(
                "You already have a subscription.".into(),
            ));
        }
    }

    // Get the Pro price (standard $20 for new signups)
    let price_id = stripe_price_id_for_plan("pro")?;

    // Get or create Stripe customer
    let customer_id = if let Some(cid) = account.as_ref().and_then(|a| a.stripe_customer_id.as_deref()) {
        cid.to_string()
    } else {
        let customer = stripe
            .create_customer(
                &auth.user_id.to_string(),
                body.billing_details.email.as_deref(),
                body.billing_details.name.as_deref(),
            )
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to create customer: {e}")))?;

        // Save customer ID
        let mut acc = account.unwrap_or_else(|| z_billing_core::Account::new(auth.user_id));
        acc.stripe_customer_id = Some(customer.id.clone());
        acc.updated_at = chrono::Utc::now();
        state.store.put_account(&acc)?;

        customer.id
    };

    // Attach payment method and set as default
    stripe
        .attach_payment_method(&body.payment_method_id, &customer_id)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to attach payment method: {e}")))?;

    // Create subscription with expanded payment intent
    let sub = stripe
        .create_inline_subscription(
            &customer_id,
            &price_id,
            &body.payment_method_id,
            &auth.user_id.to_string(),
        )
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to create subscription: {e}")))?;

    let subscription_id = sub.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let status = sub.get("status").and_then(|v| v.as_str()).unwrap_or("incomplete").to_string();

    // Extract client_secret from expanded latest_invoice.payment_intent
    let client_secret = sub
        .get("latest_invoice")
        .and_then(|inv| inv.get("payment_intent"))
        .and_then(|pi| pi.get("client_secret"))
        .and_then(|cs| cs.as_str())
        .map(|s| s.to_string());

    tracing::info!(
        user_id = %auth.user_id,
        subscription_id = %subscription_id,
        "zOS Zero Pro subscription created"
    );

    Ok(Json(ZosSubscribeResponse {
        subscription_id,
        client_secret,
        status,
    }))
}

/// Get subscription status (zos-compatible response shape).
pub async fn status_zero_pro(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<ZosStatusResponse>, ApiError> {
    let account = state.store.get_account(&auth.user_id)?;

    let subscription = account
        .and_then(|acc| {
            acc.subscription.map(|sub| {
                let status = match sub.status {
                    z_billing_core::SubscriptionStatus::Active => "active",
                    z_billing_core::SubscriptionStatus::Cancelled => "cancelled",
                    z_billing_core::SubscriptionStatus::PastDue => "past_due",
                };

                ZosSubscriptionInfo {
                    status: status.to_string(),
                    sub_type: "ZERO".to_string(),
                    stripe_subscription_id: sub.stripe_subscription_id.unwrap_or_default(),
                    current_period_end: Some(sub.current_period_end.to_rfc3339()),
                }
            })
        });

    Ok(Json(ZosStatusResponse { subscription }))
}

/// Cancel subscription at end of billing period (zos-compatible).
pub async fn cancel_zero_pro(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<ZosCancelResponse>, ApiError> {
    let account = state
        .store
        .get_account(&auth.user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    let sub = account
        .subscription
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("No active subscription".into()))?;

    let sub_id = sub
        .stripe_subscription_id
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("No Stripe subscription ID".into()))?;

    let stripe = state.stripe.as_ref().ok_or_else(|| {
        ApiError::Internal("Stripe not configured".into())
    })?;

    stripe
        .cancel_subscription_at_period_end(sub_id)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to cancel: {e}")))?;

    tracing::info!(
        user_id = %auth.user_id,
        subscription_id = %sub_id,
        "zOS subscription cancelled at period end"
    );

    Ok(Json(ZosCancelResponse { cancelled: true }))
}
