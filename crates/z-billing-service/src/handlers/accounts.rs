//! Account management handlers.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use z_billing_core::Account;
use z_billing_store::Store;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::lago::{BillingConfiguration, CustomerInput};
use crate::state::AppState;

/// Account response.
#[derive(Debug, Serialize)]
pub struct AccountResponse {
    /// User ID.
    pub user_id: String,
    /// Current balance in cents.
    pub balance_cents: i64,
    /// Balance formatted as dollars.
    pub balance_formatted: String,
    /// Lifetime purchased in cents.
    pub lifetime_purchased_cents: i64,
    /// Lifetime granted in cents.
    pub lifetime_granted_cents: i64,
    /// Lifetime used in cents.
    pub lifetime_used_cents: i64,
    /// Current plan.
    pub plan: String,
    /// Whether auto-refill is enabled.
    pub auto_refill_enabled: bool,
    /// Created timestamp.
    pub created_at: String,
}

impl From<&Account> for AccountResponse {
    #[allow(clippy::cast_precision_loss)]
    fn from(account: &Account) -> Self {
        Self {
            user_id: account.user_id.to_string(),
            balance_cents: account.balance_cents,
            balance_formatted: format!("${:.2}", account.balance_cents as f64 / 100.0),
            lifetime_purchased_cents: account.lifetime_purchased_cents,
            lifetime_granted_cents: account.lifetime_granted_cents,
            lifetime_used_cents: account.lifetime_used_cents,
            plan: format!("{:?}", account.current_plan()).to_lowercase(),
            auto_refill_enabled: account.auto_refill.as_ref().is_some_and(|a| a.enabled),
            created_at: account.created_at.to_rfc3339(),
        }
    }
}

/// Create account request (optional fields for metadata).
#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    /// Optional email (may be synced from ZID later).
    pub email: Option<String>,
}

/// Create or register a new account.
pub async fn create_account(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateAccountRequest>,
) -> Result<Json<AccountResponse>, ApiError> {
    // Check if account already exists
    if state.store.get_account(&auth.user_id)?.is_some() {
        return Err(ApiError::Conflict("Account already exists".into()));
    }

    // Create new account
    let mut account = Account::new(auth.user_id);

    // Create customer in Stripe if configured
    if let Some(stripe) = &state.stripe {
        let name = body
            .email
            .clone()
            .unwrap_or_else(|| format!("User {}", auth.user_id));

        match stripe
            .create_customer(&auth.user_id.to_string(), body.email.as_deref(), Some(&name))
            .await
        {
            Ok(customer) => {
                tracing::info!(
                    user_id = %auth.user_id,
                    stripe_id = %customer.id,
                    "Stripe customer created"
                );
                account.stripe_customer_id = Some(customer.id);
            }
            Err(e) => {
                tracing::warn!(
                    user_id = %auth.user_id,
                    error = %e,
                    "Failed to create Stripe customer - continuing without"
                );
            }
        }
    }

    // Create customer in Lago if configured
    if let Some(lago) = &state.lago {
        let customer_input = CustomerInput {
            external_id: auth.user_id.to_string(),
            name: body
                .email
                .clone()
                .unwrap_or_else(|| format!("User {}", auth.user_id)),
            email: body.email.clone(),
            billing_configuration: Some(BillingConfiguration {
                payment_provider: Some("stripe".to_string()),
                provider_customer_id: account.stripe_customer_id.clone(),
                sync_with_provider: Some(true),
            }),
            metadata: None,
        };

        match lago.create_customer(customer_input).await {
            Ok(customer) => {
                tracing::info!(
                    user_id = %auth.user_id,
                    lago_id = %customer.lago_id,
                    "Lago customer created"
                );
                account.lago_customer_id = Some(customer.lago_id);
            }
            Err(e) => {
                tracing::warn!(
                    user_id = %auth.user_id,
                    error = %e,
                    "Failed to create Lago customer - continuing without"
                );
            }
        }
    }

    state.store.put_account(&account)?;

    tracing::info!(user_id = %auth.user_id, "Account created");

    Ok(Json(AccountResponse::from(&account)))
}

/// Get the current user's account.
pub async fn get_account(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<AccountResponse>, ApiError> {
    let account = state
        .store
        .get_account(&auth.user_id)?
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    Ok(Json(AccountResponse::from(&account)))
}

/// Delete the current user's account.
pub async fn delete_account(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.store.delete_account(&auth.user_id)?;

    tracing::info!(user_id = %auth.user_id, "Account deleted");

    Ok(Json(serde_json::json!({ "deleted": true })))
}
