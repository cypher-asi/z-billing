//! Credit transaction types for z-billing.
//!
//! This module defines credit transactions that track all balance changes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{TransactionId, UserId};

/// A credit transaction representing a balance change.
///
/// All changes to an account's balance create a transaction record.
/// Transactions use ULIDs for time-ordered IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditTransaction {
    /// Unique transaction ID (ULID for time-ordering).
    pub id: TransactionId,

    /// The user whose balance was affected.
    pub user_id: UserId,

    /// Amount in cents. Positive = credit, Negative = debit.
    pub amount_cents: i64,

    /// Type of transaction.
    pub transaction_type: TransactionType,

    /// Balance after this transaction (in cents).
    pub balance_after_cents: i64,

    /// Human-readable description.
    pub description: String,

    /// Additional metadata (source, `agent_id`, model, etc.).
    pub metadata: serde_json::Value,

    /// When the transaction was created.
    pub created_at: DateTime<Utc>,
}

impl CreditTransaction {
    /// Create a new purchase transaction.
    #[must_use]
    pub fn purchase(
        user_id: UserId,
        amount_cents: i64,
        balance_after_cents: i64,
        description: String,
    ) -> Self {
        Self {
            id: TransactionId::generate(),
            user_id,
            amount_cents,
            transaction_type: TransactionType::Purchase,
            balance_after_cents,
            description,
            metadata: serde_json::Value::Null,
            created_at: Utc::now(),
        }
    }

    /// Create a new usage transaction (deduction).
    #[must_use]
    pub fn usage(
        user_id: UserId,
        amount_cents: i64,
        balance_after_cents: i64,
        description: String,
        metadata: serde_json::Value,
    ) -> Self {
        Self {
            id: TransactionId::generate(),
            user_id,
            amount_cents: -amount_cents.abs(), // Always negative for usage
            transaction_type: TransactionType::Usage,
            balance_after_cents,
            description,
            metadata,
            created_at: Utc::now(),
        }
    }

    /// Create a new subscription grant transaction.
    #[must_use]
    pub fn subscription_grant(
        user_id: UserId,
        amount_cents: i64,
        balance_after_cents: i64,
        plan_name: &str,
    ) -> Self {
        Self {
            id: TransactionId::generate(),
            user_id,
            amount_cents,
            transaction_type: TransactionType::SubscriptionGrant,
            balance_after_cents,
            description: format!("Monthly {plan_name} plan credit grant"),
            metadata: serde_json::json!({ "plan": plan_name }),
            created_at: Utc::now(),
        }
    }

    /// Create a new refund transaction.
    #[must_use]
    pub fn refund(
        user_id: UserId,
        amount_cents: i64,
        balance_after_cents: i64,
        reason: String,
    ) -> Self {
        Self {
            id: TransactionId::generate(),
            user_id,
            amount_cents,
            transaction_type: TransactionType::Refund,
            balance_after_cents,
            description: reason,
            metadata: serde_json::Value::Null,
            created_at: Utc::now(),
        }
    }

    /// Create a new bonus transaction.
    #[must_use]
    pub fn bonus(
        user_id: UserId,
        amount_cents: i64,
        balance_after_cents: i64,
        reason: String,
    ) -> Self {
        Self {
            id: TransactionId::generate(),
            user_id,
            amount_cents,
            transaction_type: TransactionType::Bonus,
            balance_after_cents,
            description: reason,
            metadata: serde_json::Value::Null,
            created_at: Utc::now(),
        }
    }

    /// Create a new auto-refill transaction.
    #[must_use]
    pub fn auto_refill(user_id: UserId, amount_cents: i64, balance_after_cents: i64) -> Self {
        Self {
            id: TransactionId::generate(),
            user_id,
            amount_cents,
            transaction_type: TransactionType::AutoRefill,
            balance_after_cents,
            description: format!("Auto-refill of {amount_cents} credits"),
            metadata: serde_json::Value::Null,
            created_at: Utc::now(),
        }
    }
}

/// Type of credit transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionType {
    /// User purchased credits.
    Purchase,

    /// Credits deducted for usage.
    Usage,

    /// Monthly subscription credit grant.
    SubscriptionGrant,

    /// Refund issued.
    Refund,

    /// Promotional/bonus credits.
    Bonus,

    /// Automatic refill triggered.
    AutoRefill,
}

impl TransactionType {
    /// Check if this transaction type adds credits (positive balance change).
    #[must_use]
    pub const fn is_credit(&self) -> bool {
        matches!(
            self,
            Self::Purchase
                | Self::SubscriptionGrant
                | Self::Refund
                | Self::Bonus
                | Self::AutoRefill
        )
    }

    /// Check if this transaction type removes credits (negative balance change).
    #[must_use]
    pub const fn is_debit(&self) -> bool {
        matches!(self, Self::Usage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purchase_transaction() {
        let user_id = UserId::generate();
        let tx = CreditTransaction::purchase(user_id, 5000, 5000, "Purchased $50 credits".into());

        assert_eq!(tx.amount_cents, 5000);
        assert_eq!(tx.transaction_type, TransactionType::Purchase);
        assert_eq!(tx.balance_after_cents, 5000);
    }

    #[test]
    fn usage_transaction_is_negative() {
        let user_id = UserId::generate();
        let tx = CreditTransaction::usage(
            user_id,
            100,
            4900,
            "LLM usage".into(),
            serde_json::json!({"model": "claude-3-5-sonnet"}),
        );

        assert_eq!(tx.amount_cents, -100); // Negative
        assert_eq!(tx.transaction_type, TransactionType::Usage);
    }

    #[test]
    fn transaction_type_is_credit_debit() {
        assert!(TransactionType::Purchase.is_credit());
        assert!(TransactionType::SubscriptionGrant.is_credit());
        assert!(TransactionType::Refund.is_credit());
        assert!(TransactionType::Bonus.is_credit());
        assert!(TransactionType::AutoRefill.is_credit());
        assert!(!TransactionType::Usage.is_credit());

        assert!(TransactionType::Usage.is_debit());
        assert!(!TransactionType::Purchase.is_debit());
    }
}
