//! Credit balance and transaction handlers.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use z_billing_core::{
    AutoRefill, CreditTransaction, DEFAULT_AUTO_REFILL_AMOUNT_CENTS,
    DEFAULT_AUTO_REFILL_TRIGGER_CENTS,
};
use z_billing_store::Store;

use crate::auth::{AdminAuth, AuthUser};
use crate::error::ApiError;
use crate::state::AppState;
use crate::stripe::PaymentResponse;

// ============================================================================
// Constants
// ============================================================================

/// Minimum credit purchase amount in USD.
const MIN_PURCHASE_USD: f64 = 5.0;

/// Maximum credit purchase amount in USD.
const MAX_PURCHASE_USD: f64 = 1000.0;

/// Minimum auto-refill trigger threshold in cents ($1).
const MIN_AUTO_REFILL_TRIGGER_CENTS: i64 = 100;

/// Minimum auto-refill amount in cents ($5).
const MIN_AUTO_REFILL_AMOUNT_CENTS: i64 = 500;

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
#[allow(clippy::cast_precision_loss)]
pub async fn get_balance(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<BalanceResponse>, ApiError> {
    let mut account = state
        .store
        .get_account(&auth.user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    // Lazy monthly allowance: if not granted in the last 30 days, issue monthly credits
    if let Some(new_balance) =
        try_monthly_allowance(state.store.as_ref(), &state.balance_tx, &account)?
    {
        account.balance_cents = new_balance;
        // Re-read account to get updated last_monthly_grant_at for daily check
        if let Some(refreshed) = state.store.get_account(&auth.user_id)? {
            account = refreshed;
        }
    }

    // Lazy daily grant: if not yet granted today, issue daily credits
    if let Some(new_balance) =
        try_daily_grant(state.store.as_ref(), &state.balance_tx, &account)?
    {
        account.balance_cents = new_balance;
    }

    Ok(Json(BalanceResponse {
        balance_cents: account.balance_cents,
        #[allow(clippy::cast_precision_loss)]
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
#[allow(clippy::cast_precision_loss)]
pub async fn purchase_credits(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<PurchaseCreditsRequest>,
) -> Result<Json<PurchaseCreditsResponse>, ApiError> {
    // Validate amount
    if body.amount_usd < MIN_PURCHASE_USD {
        return Err(ApiError::BadRequest(format!(
            "Minimum purchase is ${MIN_PURCHASE_USD}"
        )));
    }
    if body.amount_usd > MAX_PURCHASE_USD {
        return Err(ApiError::BadRequest(format!(
            "Maximum purchase is ${MAX_PURCHASE_USD}"
        )));
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

    // Convert to cents (no tier discount — 20% markup across the board)
    #[allow(clippy::cast_possible_truncation)]
    let amount_cents = (body.amount_usd * 100.0).round() as i64;

    // Credits are 1:1 with cents
    #[allow(clippy::cast_possible_truncation)]
    let credits_amount = (body.amount_usd * 100.0).round() as i64;

    tracing::info!(
        user_id = %auth.user_id,
        amount_usd = %body.amount_usd,
        amount_cents = %amount_cents,
        credits_amount = %credits_amount,
        "Initiating credit purchase"
    );

    // Create Stripe checkout session
    let success_url = format!(
        "{}/credits/success?session_id={{CHECKOUT_SESSION_ID}}",
        state.config.frontend_url
    );
    let cancel_url = format!("{}/credits/cancel", state.config.frontend_url);

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
        if trigger < MIN_AUTO_REFILL_TRIGGER_CENTS {
            return Err(ApiError::BadRequest(format!(
                "Trigger threshold must be at least {MIN_AUTO_REFILL_TRIGGER_CENTS} cents (${})",
                MIN_AUTO_REFILL_TRIGGER_CENTS / 100
            )));
        }
    }
    if let Some(amount) = body.refill_amount_cents {
        if amount < MIN_AUTO_REFILL_AMOUNT_CENTS {
            return Err(ApiError::BadRequest(format!(
                "Refill amount must be at least {MIN_AUTO_REFILL_AMOUNT_CENTS} cents (${})",
                MIN_AUTO_REFILL_AMOUNT_CENTS / 100
            )));
        }
    }

    account.auto_refill = Some(AutoRefill {
        enabled: body.enabled,
        trigger_below_cents: body
            .trigger_below_cents
            .unwrap_or(DEFAULT_AUTO_REFILL_TRIGGER_CENTS),
        refill_amount_cents: body
            .refill_amount_cents
            .unwrap_or(DEFAULT_AUTO_REFILL_AMOUNT_CENTS),
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
///
/// Requires `X-Admin-Key` header for authentication.
pub async fn admin_add_credits(
    State(state): State<Arc<AppState>>,
    admin: AdminAuth,
    Json(body): Json<AdminAddCreditsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = body
        .user_id
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid user ID".into()))?;

    if body.amount_cents <= 0 {
        return Err(ApiError::BadRequest(
            "amount_cents must be positive".into(),
        ));
    }

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

    // Broadcast balance update to WebSocket clients
    #[allow(clippy::cast_precision_loss)]
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
        admin_id = %admin.admin_id,
        user_id = %user_id,
        amount_cents = %body.amount_cents,
        reason = %body.reason,
        new_balance = %balance,
        "Admin added credits"
    );

    Ok(Json(serde_json::json!({
        "balance_cents": balance,
        "transaction_id": tx.id.to_string()
    })))
}

// ============================================================================
// Signup Grant
// ============================================================================

/// Default signup grant amount if env var is not set (5000 credits = $50).
fn signup_grant_amount() -> i64 {
    std::env::var("SIGNUP_GRANT_CREDITS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5000)
}

/// Request body for the signup grant endpoint.
#[derive(Debug, Deserialize)]
pub struct SignupGrantRequest {
    /// The user ID to grant signup credits to.
    pub user_id: String,
    /// Whether this user has a ZERO Pro subscription.
    #[serde(default)]
    pub is_zero_pro: bool,
}

/// Grant one-time signup credits to a new user.
///
/// Idempotent: if the user has already received a signup grant, returns
/// `granted: false`. Uses `signup_grant_at` timestamp on the account for
/// O(1) duplicate detection.
///
/// Requires service-to-service auth via `X-API-Key` header.
pub async fn signup_grant(
    State(state): State<Arc<AppState>>,
    _auth: crate::auth::ServiceAuth,
    Json(body): Json<SignupGrantRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = body
        .user_id
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid user ID".into()))?;

    let amount = signup_grant_amount();

    // Get or create account
    let account = match state.store.get_account(&user_id)? {
        Some(a) => a,
        None => {
            let new_account = z_billing_core::Account::new(user_id);
            state.store.put_account(&new_account)?;
            new_account
        }
    };

    // Check if already granted
    if account.signup_grant_at.is_some() {
        return Ok(Json(serde_json::json!({
            "granted": false,
            "reason": "already_granted",
            "balance_cents": account.balance_cents,
        })));
    }

    // Grant credits
    let new_balance = account.balance_cents + amount;
    let tx = CreditTransaction::signup_grant(user_id, amount, new_balance);

    let balance = state.store.add_credits(&user_id, amount, &tx)?;

    // Mark signup grant as issued and store Zero Pro status
    let mut updated = state.store.get_account(&user_id)?.unwrap_or(account);
    updated.signup_grant_at = Some(chrono::Utc::now());
    updated.is_zero_pro = body.is_zero_pro;
    updated.updated_at = chrono::Utc::now();
    state.store.put_account(&updated)?;

    // Broadcast balance update
    #[allow(clippy::cast_precision_loss)]
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
        amount_cents = %amount,
        new_balance = %balance,
        "Signup credit grant issued"
    );

    Ok(Json(serde_json::json!({
        "granted": true,
        "amount_cents": amount,
        "balance_cents": balance,
    })))
}

// ============================================================================
// Daily Grant
// ============================================================================

/// Daily grant amount based on plan tier.
fn daily_grant_amount(plan: &z_billing_core::Plan) -> i64 {
    let normalized = plan.normalized();
    let env_key = match normalized {
        z_billing_core::Plan::Mortal => "DAILY_GRANT_MORTAL",
        z_billing_core::Plan::Pro => "DAILY_GRANT_PRO",
        z_billing_core::Plan::Crusader => "DAILY_GRANT_CRUSADER",
        z_billing_core::Plan::Sage => "DAILY_GRANT_SAGE",
        _ => "DAILY_GRANT_MORTAL",
    };
    let default = match normalized {
        z_billing_core::Plan::Mortal => 50,       // $0.50
        z_billing_core::Plan::Pro => 100,         // $1.00
        z_billing_core::Plan::Crusader => 200,    // $2.00
        z_billing_core::Plan::Sage => 400,        // $4.00
        _ => 50,
    };
    std::env::var(env_key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Check if the account is eligible for a daily grant and issue it if so.
///
/// Returns the new balance if a grant was issued, or None if not eligible.
/// Uses the lazy approach: credits are granted on first use of the day.
/// Previous days' unused daily credits are NOT carried over — you only get
/// today's grant when you show up.
///
/// This function is safe to call from multiple code paths — the
/// `last_daily_grant_at` check prevents double-grants within the same day.
pub fn try_daily_grant(
    store: &dyn Store,
    balance_tx: &tokio::sync::broadcast::Sender<String>,
    account: &z_billing_core::Account,
) -> Result<Option<i64>, ApiError> {
    let today = chrono::Utc::now().date_naive();

    // Check if already granted today
    if let Some(last_grant) = account.last_daily_grant_at {
        if last_grant.date_naive() >= today {
            return Ok(None);
        }
    }

    let plan = account.current_plan();
    let amount = daily_grant_amount(&plan);
    if amount <= 0 {
        return Ok(None);
    }

    let user_id = account.user_id;
    let new_balance = account.balance_cents + amount;
    let tx = CreditTransaction::daily_grant(user_id, amount, new_balance);

    let balance = store.add_credits(&user_id, amount, &tx)?;

    // Update last_daily_grant_at
    let mut updated = store.get_account(&user_id)?.unwrap_or_else(|| account.clone());
    updated.last_daily_grant_at = Some(chrono::Utc::now());
    updated.updated_at = chrono::Utc::now();
    store.put_account(&updated)?;

    // Broadcast balance update
    #[allow(clippy::cast_precision_loss)]
    let _ = balance_tx.send(
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
        amount_cents = %amount,
        new_balance = %balance,
        "Daily credit grant issued"
    );

    Ok(Some(balance))
}

/// Check if the account is eligible for a monthly credit allowance and issue it if so.
///
/// Returns the new balance if a grant was issued, or None if not eligible.
/// Checks `last_monthly_grant_at` — grants if it's been more than 30 days.
pub fn try_monthly_allowance(
    store: &dyn Store,
    balance_tx: &tokio::sync::broadcast::Sender<String>,
    account: &z_billing_core::Account,
) -> Result<Option<i64>, ApiError> {
    let now = chrono::Utc::now();

    // Check if already granted this month (within last 30 days)
    if let Some(last_grant) = account.last_monthly_grant_at {
        let days_since = (now - last_grant).num_days();
        if days_since < 30 {
            return Ok(None);
        }
    }

    let plan = account.current_plan();
    let amount = plan.monthly_credits();
    if amount <= 0 {
        return Ok(None);
    }

    let user_id = account.user_id;
    let new_balance = account.balance_cents + amount;
    let tx = CreditTransaction::monthly_allowance(user_id, amount, new_balance);

    let balance = store.add_credits(&user_id, amount, &tx)?;

    // Update last_monthly_grant_at
    let mut updated = store.get_account(&user_id)?.unwrap_or_else(|| account.clone());
    updated.last_monthly_grant_at = Some(now);
    updated.updated_at = now;
    store.put_account(&updated)?;

    // Broadcast balance update
    #[allow(clippy::cast_precision_loss)]
    let _ = balance_tx.send(
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
        amount_cents = %amount,
        new_balance = %balance,
        "Monthly credit allowance granted"
    );

    Ok(Some(balance))
}

/// Request body for the daily grant endpoint.
#[derive(Debug, Deserialize)]
pub struct DailyGrantRequest {
    /// The user ID to grant daily credits to.
    pub user_id: String,
}

/// Explicitly trigger a daily grant for a user.
///
/// Idempotent — safe to call multiple times per day.
/// Requires service-to-service auth via `X-API-Key` header.
pub async fn daily_grant(
    State(state): State<Arc<AppState>>,
    _auth: crate::auth::ServiceAuth,
    Json(body): Json<DailyGrantRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = body
        .user_id
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid user ID".into()))?;

    let account = state
        .store
        .get_account(&user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    match try_daily_grant(state.store.as_ref(), &state.balance_tx, &account)? {
        Some(balance) => Ok(Json(serde_json::json!({
            "granted": true,
            "amount_cents": daily_grant_amount(&account.current_plan()),
            "balance_cents": balance,
        }))),
        None => Ok(Json(serde_json::json!({
            "granted": false,
            "reason": "already_granted_today",
            "balance_cents": account.balance_cents,
        }))),
    }
}

// ============================================================================
// Referral Grant
// ============================================================================

/// Referral grant amount for the invitee (always the same regardless of tier).
fn referral_invitee_amount() -> i64 {
    std::env::var("REFERRAL_GRANT_INVITEE_CREDITS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5000)
}

/// Referral grant amount for the inviter, scaled by their tier.
fn referral_inviter_amount(plan: &z_billing_core::Plan) -> i64 {
    let normalized = plan.normalized();
    let env_key = match normalized {
        z_billing_core::Plan::Mortal => "REFERRAL_GRANT_INVITER_MORTAL",
        z_billing_core::Plan::Pro => "REFERRAL_GRANT_INVITER_PRO",
        z_billing_core::Plan::Crusader => "REFERRAL_GRANT_INVITER_CRUSADER",
        z_billing_core::Plan::Sage => "REFERRAL_GRANT_INVITER_SAGE",
        _ => "REFERRAL_GRANT_INVITER_MORTAL",
    };
    let default = match normalized {
        z_billing_core::Plan::Mortal => 5000,     // $50
        z_billing_core::Plan::Pro => 7500,        // $75
        z_billing_core::Plan::Crusader => 10000,  // $100
        z_billing_core::Plan::Sage => 15000,      // $150
        _ => 5000,
    };
    std::env::var(env_key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Request body for the referral grant endpoint.
#[derive(Debug, Deserialize)]
pub struct ReferralGrantRequest {
    /// The user ID of the person who shared the invite code.
    pub inviter_user_id: String,
    /// The user ID of the new user who signed up with the code.
    pub invitee_user_id: String,
}

/// Grant referral credits to both inviter and invitee.
///
/// Idempotent: if this invitee has already received a referral bonus,
/// returns `granted: false`. Both grants happen in sequence — invitee
/// is checked first to prevent duplicate grants.
///
/// The inviter's bonus scales by their tier (Mortal=$50, Pro=$75,
/// Crusader=$100, Sage=$150), incentivising tier upgrades.
///
/// Requires service-to-service auth via `X-API-Key` header.
pub async fn referral_grant(
    State(state): State<Arc<AppState>>,
    _auth: crate::auth::ServiceAuth,
    Json(body): Json<ReferralGrantRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let inviter_id: z_billing_core::UserId = body
        .inviter_user_id
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid inviter user ID".into()))?;
    let invitee_id: z_billing_core::UserId = body
        .invitee_user_id
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid invitee user ID".into()))?;

    if inviter_id == invitee_id {
        return Err(ApiError::BadRequest("Inviter and invitee cannot be the same user".into()));
    }

    // Get or create invitee account
    let invitee_account = match state.store.get_account(&invitee_id)? {
        Some(a) => a,
        None => {
            let new_account = z_billing_core::Account::new(invitee_id);
            state.store.put_account(&new_account)?;
            new_account
        }
    };

    // Check if invitee already received a referral bonus by checking transactions
    let invitee_txs = state.store.list_transactions_by_user(&invitee_id, 100, 0)?;
    let already_granted = invitee_txs
        .iter()
        .any(|t| t.transaction_type == z_billing_core::TransactionType::ReferralBonus);

    if already_granted {
        return Ok(Json(serde_json::json!({
            "granted": false,
            "reason": "already_granted",
        })));
    }

    let invitee_amount = referral_invitee_amount();

    // Grant to invitee
    let invitee_new_balance = invitee_account.balance_cents + invitee_amount;
    let invitee_tx = CreditTransaction::referral_bonus(
        invitee_id,
        invitee_amount,
        invitee_new_balance,
        format!("Referral bonus — invited by {}", body.inviter_user_id),
    );
    let invitee_balance = state.store.add_credits(&invitee_id, invitee_amount, &invitee_tx)?;

    // Broadcast invitee balance update
    #[allow(clippy::cast_precision_loss)]
    let _ = state.balance_tx.send(
        serde_json::json!({
            "type": "balance.updated",
            "userId": invitee_id.to_string(),
            "balanceCents": invitee_balance,
            "balanceFormatted": format!("${:.2}", invitee_balance as f64 / 100.0),
        })
        .to_string(),
    );

    // Get or create inviter account and determine bonus amount
    let inviter_account = match state.store.get_account(&inviter_id)? {
        Some(a) => a,
        None => {
            let new_account = z_billing_core::Account::new(inviter_id);
            state.store.put_account(&new_account)?;
            new_account
        }
    };

    let inviter_amount = referral_inviter_amount(&inviter_account.current_plan());

    // Grant to inviter
    let inviter_new_balance = inviter_account.balance_cents + inviter_amount;
    let inviter_tx = CreditTransaction::referral_bonus(
        inviter_id,
        inviter_amount,
        inviter_new_balance,
        format!("Referral bonus — {} signed up with your invite", body.invitee_user_id),
    );
    let inviter_balance = state.store.add_credits(&inviter_id, inviter_amount, &inviter_tx)?;

    // Broadcast inviter balance update
    #[allow(clippy::cast_precision_loss)]
    let _ = state.balance_tx.send(
        serde_json::json!({
            "type": "balance.updated",
            "userId": inviter_id.to_string(),
            "balanceCents": inviter_balance,
            "balanceFormatted": format!("${:.2}", inviter_balance as f64 / 100.0),
        })
        .to_string(),
    );

    tracing::info!(
        inviter_id = %inviter_id,
        invitee_id = %invitee_id,
        invitee_amount = %invitee_amount,
        inviter_amount = %inviter_amount,
        "Referral credit grant issued to both parties"
    );

    Ok(Json(serde_json::json!({
        "granted": true,
        "invitee_amount_cents": invitee_amount,
        "inviter_amount_cents": inviter_amount,
        "invitee_balance_cents": invitee_balance,
        "inviter_balance_cents": inviter_balance,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use z_billing_core::Plan;

    // Env var tests must run sequentially to avoid pollution
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn signup_grant_amount_defaults_to_5000() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::remove_var("SIGNUP_GRANT_CREDITS");
        assert_eq!(signup_grant_amount(), 5000);
    }

    #[test]
    fn signup_grant_amount_reads_env_var() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::set_var("SIGNUP_GRANT_CREDITS", "10000");
        assert_eq!(signup_grant_amount(), 10000);
        std::env::remove_var("SIGNUP_GRANT_CREDITS");
    }

    #[test]
    fn daily_grant_amount_defaults() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::remove_var("DAILY_GRANT_MORTAL");
        std::env::remove_var("DAILY_GRANT_PRO");
        std::env::remove_var("DAILY_GRANT_CRUSADER");
        std::env::remove_var("DAILY_GRANT_SAGE");

        assert_eq!(daily_grant_amount(&Plan::Mortal), 50);
        assert_eq!(daily_grant_amount(&Plan::Pro), 100);
        assert_eq!(daily_grant_amount(&Plan::Crusader), 200);
        assert_eq!(daily_grant_amount(&Plan::Sage), 400);
    }

    #[test]
    fn daily_grant_amount_legacy_plans_normalize() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::remove_var("DAILY_GRANT_MORTAL");
        std::env::remove_var("DAILY_GRANT_PRO");

        // Legacy Free maps to Mortal
        assert_eq!(daily_grant_amount(&Plan::Free), 50);
        // Legacy Standard maps to Pro
        assert_eq!(daily_grant_amount(&Plan::Standard), 100);
    }

    #[test]
    fn daily_grant_amount_reads_env_vars() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::remove_var("DAILY_GRANT_MORTAL");
        std::env::set_var("DAILY_GRANT_MORTAL", "75");
        assert_eq!(daily_grant_amount(&Plan::Mortal), 75);
        std::env::remove_var("DAILY_GRANT_MORTAL");
    }

    #[test]
    fn daily_grant_amount_ignores_invalid_env_var() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::set_var("DAILY_GRANT_MORTAL", "not_a_number");
        assert_eq!(daily_grant_amount(&Plan::Mortal), 50);
        std::env::remove_var("DAILY_GRANT_MORTAL");
    }

    #[test]
    fn referral_invitee_amount_defaults_to_5000() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::remove_var("REFERRAL_GRANT_INVITEE_CREDITS");
        assert_eq!(referral_invitee_amount(), 5000);
    }

    #[test]
    fn referral_inviter_amount_scales_by_tier() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::remove_var("REFERRAL_GRANT_INVITER_MORTAL");
        std::env::remove_var("REFERRAL_GRANT_INVITER_PRO");
        std::env::remove_var("REFERRAL_GRANT_INVITER_CRUSADER");
        std::env::remove_var("REFERRAL_GRANT_INVITER_SAGE");

        assert_eq!(referral_inviter_amount(&Plan::Mortal), 5000);
        assert_eq!(referral_inviter_amount(&Plan::Pro), 7500);
        assert_eq!(referral_inviter_amount(&Plan::Crusader), 10000);
        assert_eq!(referral_inviter_amount(&Plan::Sage), 15000);
    }
}
