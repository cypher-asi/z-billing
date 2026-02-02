//! Credit balance and transaction handlers.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use z_billing_core::{AutoRefill, CreditTransaction};
use z_billing_store::Store;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;
use crate::stripe::PaymentResponse;

/// Balance response.
#[derive(Debug, Serialize)]
pub struct BalanceResponse {
    /// Balance in cents (Z Credits).
    pub balance_cents: i64,
    /// Balance formatted as dollars.
    pub balance_formatted: String,
    /// Current plan.
    pub plan: String,
}

/// Get current credit balance.
pub async fn get_balance(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<BalanceResponse>, ApiError> {
    let account = state
        .store
        .get_account(&auth.user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    Ok(Json(BalanceResponse {
        balance_cents: account.balance_cents,
        balance_formatted: format!("${:.2}", account.balance_cents as f64 / 100.0),
        plan: format!("{:?}", account.current_plan()).to_lowercase(),
    }))
}

/// Transaction list query parameters.
#[derive(Debug, Deserialize)]
pub struct ListTransactionsQuery {
    /// Maximum number of transactions to return (default: 50).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Offset for pagination (default: 0).
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    50
}

/// Transaction response.
#[derive(Debug, Serialize)]
pub struct TransactionResponse {
    /// Transaction ID.
    pub id: String,
    /// Amount in cents (positive = credit, negative = debit).
    pub amount_cents: i64,
    /// Transaction type.
    pub transaction_type: String,
    /// Balance after this transaction.
    pub balance_after_cents: i64,
    /// Description.
    pub description: String,
    /// Timestamp.
    pub created_at: String,
}

impl From<&CreditTransaction> for TransactionResponse {
    fn from(tx: &CreditTransaction) -> Self {
        Self {
            id: tx.id.to_string(),
            amount_cents: tx.amount_cents,
            transaction_type: format!("{:?}", tx.transaction_type).to_lowercase(),
            balance_after_cents: tx.balance_after_cents,
            description: tx.description.clone(),
            created_at: tx.created_at.to_rfc3339(),
        }
    }
}

/// List transactions response.
#[derive(Debug, Serialize)]
pub struct ListTransactionsResponse {
    /// Transactions (newest first).
    pub transactions: Vec<TransactionResponse>,
    /// Whether there are more transactions.
    pub has_more: bool,
}

/// List transaction history.
pub async fn list_transactions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<ListTransactionsQuery>,
) -> Result<Json<ListTransactionsResponse>, ApiError> {
    // Verify account exists
    state
        .store
        .get_account(&auth.user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    // Fetch one more than requested to determine has_more
    let limit = query.limit.min(100);
    let transactions =
        state
            .store
            .list_transactions_by_user(&auth.user_id, limit + 1, query.offset)?;

    let has_more = transactions.len() > limit;
    let transactions: Vec<_> = transactions
        .iter()
        .take(limit)
        .map(TransactionResponse::from)
        .collect();

    Ok(Json(ListTransactionsResponse {
        transactions,
        has_more,
    }))
}

/// Purchase credits request.
#[derive(Debug, Deserialize)]
pub struct PurchaseCreditsRequest {
    /// Amount in dollars to purchase.
    pub amount_usd: f64,
}

/// Purchase credits response.
#[derive(Debug, Serialize)]
pub struct PurchaseCreditsResponse {
    /// Stripe checkout session URL.
    pub checkout_url: String,
    /// Session ID for tracking.
    pub session_id: String,
}

/// Initiate a credit purchase via Stripe.
pub async fn purchase_credits(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<PurchaseCreditsRequest>,
) -> Result<Json<PurchaseCreditsResponse>, ApiError> {
    // Validate amount
    if body.amount_usd < 5.0 {
        return Err(ApiError::BadRequest("Minimum purchase is $5".into()));
    }
    if body.amount_usd > 1000.0 {
        return Err(ApiError::BadRequest("Maximum purchase is $1000".into()));
    }

    // Verify Stripe is configured
    let stripe = state
        .stripe
        .as_ref()
        .ok_or_else(|| ApiError::ExternalService("Stripe not configured".into()))?;

    // Verify account exists
    let account = state
        .store
        .get_account(&auth.user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    // Apply plan discount
    let discount_percent = account.current_plan().purchase_discount_percent();
    let final_amount = body.amount_usd * (1.0 - f64::from(discount_percent) / 100.0);

    // Convert to cents
    #[allow(clippy::cast_possible_truncation)]
    let amount_cents = (final_amount * 100.0).round() as i64;

    // Credits are 1:1 with cents for the original amount (before discount)
    #[allow(clippy::cast_possible_truncation)]
    let credits_amount = (body.amount_usd * 100.0).round() as i64;

    tracing::info!(
        user_id = %auth.user_id,
        amount_usd = %body.amount_usd,
        discount_percent = %discount_percent,
        final_amount = %final_amount,
        amount_cents = %amount_cents,
        credits_amount = %credits_amount,
        "Initiating credit purchase"
    );

    // Create Stripe checkout session
    let success_url = format!(
        "{}/billing/success?session_id={{CHECKOUT_SESSION_ID}}",
        state.config.frontend_url
    );
    let cancel_url = format!("{}/billing/cancel", state.config.frontend_url);

    let session = stripe
        .create_checkout_session(
            account.stripe_customer_id.as_deref(),
            &auth.user_id.to_string(),
            amount_cents,
            credits_amount,
            &success_url,
            &cancel_url,
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to create Stripe checkout session");
            ApiError::ExternalService(format!("Failed to create checkout session: {e}"))
        })?;

    let checkout_url = session
        .url
        .ok_or_else(|| ApiError::ExternalService("Stripe returned no checkout URL".into()))?;

    tracing::info!(
        user_id = %auth.user_id,
        session_id = %session.id,
        "Stripe checkout session created"
    );

    Ok(Json(PurchaseCreditsResponse {
        checkout_url,
        session_id: session.id,
    }))
}

/// Auto-refill configuration request.
#[derive(Debug, Deserialize)]
pub struct AutoRefillRequest {
    /// Whether to enable auto-refill.
    pub enabled: bool,
    /// Trigger when balance drops below this (in cents).
    pub trigger_below_cents: Option<i64>,
    /// Amount to refill (in cents).
    pub refill_amount_cents: Option<i64>,
}

/// Configure auto-refill settings.
pub async fn configure_auto_refill(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<AutoRefillRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut account = state
        .store
        .get_account(&auth.user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    // Validate amounts
    if let Some(trigger) = body.trigger_below_cents {
        if trigger < 100 {
            return Err(ApiError::BadRequest(
                "Trigger threshold must be at least 100 cents ($1)".into(),
            ));
        }
    }
    if let Some(amount) = body.refill_amount_cents {
        if amount < 500 {
            return Err(ApiError::BadRequest(
                "Refill amount must be at least 500 cents ($5)".into(),
            ));
        }
    }

    account.auto_refill = Some(AutoRefill {
        enabled: body.enabled,
        trigger_below_cents: body.trigger_below_cents.unwrap_or(500),
        refill_amount_cents: body.refill_amount_cents.unwrap_or(2500),
    });
    account.updated_at = chrono::Utc::now();

    state.store.put_account(&account)?;

    tracing::info!(
        user_id = %auth.user_id,
        enabled = %body.enabled,
        "Auto-refill configured"
    );

    Ok(Json(serde_json::json!({
        "auto_refill": account.auto_refill
    })))
}

/// Payment history query parameters.
#[derive(Debug, Deserialize)]
pub struct ListPaymentsQuery {
    /// Maximum number of payments to return (default: 10, max: 100).
    #[serde(default = "default_payments_limit")]
    pub limit: u32,
}

fn default_payments_limit() -> u32 {
    10
}

/// Payment history response.
#[derive(Debug, Serialize)]
pub struct ListPaymentsResponse {
    /// List of payments.
    pub payments: Vec<PaymentResponse>,
    /// Whether there are more payments.
    pub has_more: bool,
}

/// List payment history from Stripe.
pub async fn list_payments(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<ListPaymentsQuery>,
) -> Result<Json<ListPaymentsResponse>, ApiError> {
    // Verify Stripe is configured
    let stripe = state
        .stripe
        .as_ref()
        .ok_or_else(|| ApiError::ExternalService("Stripe not configured".into()))?;

    // Get account
    let account = state
        .store
        .get_account(&auth.user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    // Get Stripe customer ID
    let customer_id = account
        .stripe_customer_id
        .ok_or_else(|| ApiError::NotFound("No Stripe customer linked to account".into()))?;

    // List payment intents
    let limit = query.limit.min(100);
    let payment_list = stripe
        .list_payment_intents(&customer_id, Some(limit))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to list payment intents");
            ApiError::ExternalService(format!("Failed to fetch payment history: {e}"))
        })?;

    let payments: Vec<PaymentResponse> = payment_list
        .data
        .iter()
        .map(PaymentResponse::from)
        .collect();

    Ok(Json(ListPaymentsResponse {
        payments,
        has_more: payment_list.has_more,
    }))
}

/// Admin add credits request (for testing/bonus).
#[derive(Debug, Deserialize)]
pub struct AdminAddCreditsRequest {
    /// User ID to add credits to.
    pub user_id: String,
    /// Amount in cents.
    pub amount_cents: i64,
    /// Reason for the credit.
    pub reason: String,
}

/// Admin endpoint to add credits (bonus/promo).
pub async fn admin_add_credits(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AdminAddCreditsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = body
        .user_id
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid user ID".into()))?;

    // Get account
    let account = state
        .store
        .get_account(&user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    // Create transaction
    let new_balance = account.balance_cents + body.amount_cents;
    let tx = CreditTransaction::bonus(user_id, body.amount_cents, new_balance, body.reason.clone());

    // Add credits
    let balance = state.store.add_credits(&user_id, body.amount_cents, &tx)?;

    tracing::info!(
        user_id = %user_id,
        amount_cents = %body.amount_cents,
        reason = %body.reason,
        new_balance = %balance,
        "Credits added"
    );

    Ok(Json(serde_json::json!({
        "balance_cents": balance,
        "transaction_id": tx.id.to_string()
    })))
}
