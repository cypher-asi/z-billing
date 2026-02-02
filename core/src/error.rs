//! Error types for z-billing.

use crate::ids::IdError;

/// Result type for z-billing operations.
pub type Result<T> = std::result::Result<T, BillingError>;

/// Errors that can occur in z-billing operations.
#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    /// Insufficient credits for the operation.
    #[error("insufficient credits: balance={balance}, required={required}")]
    InsufficientCredits {
        /// Current balance in cents.
        balance: i64,
        /// Required amount in cents.
        required: i64,
    },

    /// Account not found.
    #[error("account not found: {user_id}")]
    AccountNotFound {
        /// The user ID that was not found.
        user_id: String,
    },

    /// Transaction not found.
    #[error("transaction not found: {transaction_id}")]
    TransactionNotFound {
        /// The transaction ID that was not found.
        transaction_id: String,
    },

    /// Account already exists.
    #[error("account already exists: {user_id}")]
    AccountAlreadyExists {
        /// The user ID that already exists.
        user_id: String,
    },

    /// Invalid subscription plan transition.
    #[error("invalid plan transition from {from:?} to {to:?}")]
    InvalidPlanTransition {
        /// The current plan.
        from: crate::Plan,
        /// The target plan.
        to: crate::Plan,
    },

    /// External service error (Lago, Stripe).
    #[error("external service error: {service} - {message}")]
    ExternalService {
        /// The service that failed.
        service: String,
        /// Error message.
        message: String,
    },

    /// Storage error.
    #[error("storage error: {0}")]
    Storage(String),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Invalid identifier.
    #[error("invalid identifier: {0}")]
    InvalidId(#[from] IdError),

    /// Duplicate event (idempotency).
    #[error("duplicate event: {event_id}")]
    DuplicateEvent {
        /// The event ID that was duplicated.
        event_id: String,
    },

    /// Invalid amount.
    #[error("invalid amount: {0}")]
    InvalidAmount(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    Configuration(String),
}
