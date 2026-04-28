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

    // Prevent duplicate subscriptions — if user already has an active subscription,
    // they should use the Customer Portal to change plans instead.
    if let Some(ref acc) = account {
        if acc.has_active_subscription() {
            return Err(ApiError::BadRequest(
                "You already have an active subscription. Use the Customer Portal to change plans.".into(),
            ));
        }
    }

    let success_url = format!(
        "{}/settings?subscription=success&session_id={{CHECKOUT_SESSION_ID}}",
        state.config.frontend_url
    );
    let cancel_url = format!("{}/settings?subscription=cancelled", state.config.frontend_url);

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

    Ok(Json(SubscriptionStatusResponse {
        plan: format!("{:?}", plan).to_lowercase(),
        is_subscribed,
        monthly_credits: plan.monthly_credits(),
    }))
}
