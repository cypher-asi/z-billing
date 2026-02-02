//! Account types for z-billing.
//!
//! This module defines the account structure including subscriptions and auto-refill settings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::UserId;

/// A billing account for a user.
///
/// The account tracks credit balance, subscription status, and integration IDs
/// for external services (Lago, Stripe).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// The user ID (from Zero-ID).
    pub user_id: UserId,

    /// Current Z Credit balance in cents.
    /// 1 Z Credit = $0.01 = 1 cent.
    pub balance_cents: i64,

    /// Lifetime credits purchased (in cents).
    pub lifetime_purchased_cents: i64,

    /// Lifetime credits granted from subscriptions (in cents).
    pub lifetime_granted_cents: i64,

    /// Lifetime credits used (in cents).
    pub lifetime_used_cents: i64,

    /// Current subscription, if any.
    pub subscription: Option<Subscription>,

    /// Auto-refill configuration, if enabled.
    pub auto_refill: Option<AutoRefill>,

    /// Lago customer ID for usage reporting.
    pub lago_customer_id: Option<String>,

    /// Stripe customer ID for payments.
    pub stripe_customer_id: Option<String>,

    /// When the account was created.
    pub created_at: DateTime<Utc>,

    /// When the account was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Account {
    /// Create a new account with zero balance.
    #[must_use]
    pub fn new(user_id: UserId) -> Self {
        let now = Utc::now();
        Self {
            user_id,
            balance_cents: 0,
            lifetime_purchased_cents: 0,
            lifetime_granted_cents: 0,
            lifetime_used_cents: 0,
            subscription: None,
            auto_refill: None,
            lago_customer_id: None,
            stripe_customer_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Check if the account has sufficient credits for a deduction.
    #[must_use]
    pub fn has_sufficient_credits(&self, amount_cents: i64) -> bool {
        self.balance_cents >= amount_cents
    }

    /// Get the current plan (Free if no subscription).
    #[must_use]
    pub fn current_plan(&self) -> Plan {
        self.subscription
            .as_ref()
            .map_or(Plan::Free, |s| s.plan.clone())
    }

    /// Check if the account has an active subscription.
    #[must_use]
    pub fn has_active_subscription(&self) -> bool {
        self.subscription
            .as_ref()
            .is_some_and(|s| s.status == SubscriptionStatus::Active)
    }
}

/// A subscription to a billing plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// The subscription plan.
    pub plan: Plan,

    /// Current status of the subscription.
    pub status: SubscriptionStatus,

    /// Start of the current billing period.
    pub current_period_start: DateTime<Utc>,

    /// End of the current billing period.
    pub current_period_end: DateTime<Utc>,

    /// Lago subscription ID.
    pub lago_subscription_id: String,

    /// When the subscription was created.
    pub created_at: DateTime<Utc>,
}

/// Available billing plans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    /// Free tier: $0/month, 0 credits/month, pay-as-you-go only.
    Free,

    /// Standard plan: $20/month, 2500 credits/month, 10% discount on one-time purchases.
    Standard,

    /// Pro plan: $50/month, 6000 credits/month, 20% discount on one-time purchases.
    Pro,

    /// Enterprise plan: Custom pricing, custom credits, custom discount.
    Enterprise,
}

impl Plan {
    /// Get the monthly credit allowance for this plan.
    #[must_use]
    pub const fn monthly_credits(&self) -> i64 {
        match self {
            Self::Standard => 2500,
            Self::Pro => 6000,
            Self::Free | Self::Enterprise => 0, // Free=none, Enterprise=custom (set elsewhere)
        }
    }

    /// Get the discount percentage for one-time purchases.
    #[must_use]
    pub const fn purchase_discount_percent(&self) -> u8 {
        match self {
            Self::Standard => 10,
            Self::Pro => 20,
            Self::Free | Self::Enterprise => 0, // Free=none, Enterprise=custom
        }
    }

    /// Get the monthly price in cents.
    #[must_use]
    pub const fn monthly_price_cents(&self) -> i64 {
        match self {
            Self::Standard => 2000,             // $20
            Self::Pro => 5000,                  // $50
            Self::Free | Self::Enterprise => 0, // Free=none, Enterprise=custom
        }
    }
}

/// Status of a subscription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    /// Subscription is active.
    Active,

    /// Subscription was cancelled (still active until period end).
    Cancelled,

    /// Payment failed, subscription is past due.
    PastDue,
}

/// Auto-refill configuration.
///
/// When enabled, the system will automatically purchase credits
/// when the balance drops below the threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoRefill {
    /// Whether auto-refill is enabled.
    pub enabled: bool,

    /// Trigger refill when balance drops below this amount (in cents).
    pub trigger_below_cents: i64,

    /// Amount to refill (in cents).
    pub refill_amount_cents: i64,
}

impl Default for AutoRefill {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger_below_cents: 500,  // $5
            refill_amount_cents: 2500, // $25
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_account_has_zero_balance() {
        let user_id = UserId::generate();
        let account = Account::new(user_id);
        assert_eq!(account.balance_cents, 0);
        assert_eq!(account.lifetime_purchased_cents, 0);
        assert_eq!(account.lifetime_used_cents, 0);
        assert!(account.subscription.is_none());
    }

    #[test]
    fn account_sufficient_credits() {
        let user_id = UserId::generate();
        let mut account = Account::new(user_id);
        account.balance_cents = 1000;

        assert!(account.has_sufficient_credits(500));
        assert!(account.has_sufficient_credits(1000));
        assert!(!account.has_sufficient_credits(1001));
    }

    #[test]
    fn plan_monthly_credits() {
        assert_eq!(Plan::Free.monthly_credits(), 0);
        assert_eq!(Plan::Standard.monthly_credits(), 2500);
        assert_eq!(Plan::Pro.monthly_credits(), 6000);
    }

    #[test]
    fn plan_discount_percent() {
        assert_eq!(Plan::Free.purchase_discount_percent(), 0);
        assert_eq!(Plan::Standard.purchase_discount_percent(), 10);
        assert_eq!(Plan::Pro.purchase_discount_percent(), 20);
    }
}
