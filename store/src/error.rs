//! Error types for z-billing storage.

/// Result type for storage operations.
pub type Result<T> = std::result::Result<T, StoreError>;

/// Errors that can occur in storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Database operation failed.
    #[error("database error: {0}")]
    Database(String),

    /// Serialization/deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Record not found.
    #[error("not found")]
    NotFound,

    /// Insufficient credits for deduction.
    #[error("insufficient credits: balance={balance}, required={required}")]
    InsufficientCredits {
        /// Current balance in cents.
        balance: i64,
        /// Required amount in cents.
        required: i64,
    },

    /// Duplicate event (idempotency check failed).
    #[error("duplicate event: {event_id}")]
    DuplicateEvent {
        /// The event ID that was duplicated.
        event_id: String,
    },
}
