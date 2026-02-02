//! Client error types.

/// Errors that can occur when using the z-billing client.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Server returned an error response.
    #[error("API error: {code} - {message}")]
    Api {
        /// Error code.
        code: String,
        /// Error message.
        message: String,
        /// HTTP status code.
        status: u16,
    },

    /// Insufficient credits.
    #[error("insufficient credits: balance={balance}, required={required}")]
    InsufficientCredits {
        /// Current balance.
        balance: i64,
        /// Required amount.
        required: i64,
    },

    /// Duplicate event (already processed).
    #[error("duplicate event: {event_id}")]
    DuplicateEvent {
        /// The event ID.
        event_id: String,
    },

    /// Account not found.
    #[error("account not found: {user_id}")]
    AccountNotFound {
        /// The user ID.
        user_id: String,
    },

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid configuration.
    #[error("configuration error: {0}")]
    Configuration(String),
}
