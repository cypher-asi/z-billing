//! API error types and responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// API error type.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// Unauthorized - missing or invalid credentials.
    #[error("unauthorized")]
    Unauthorized,

    /// Forbidden - valid credentials but insufficient permissions.
    #[error("forbidden")]
    Forbidden,

    /// Resource not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Bad request - invalid input.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// Conflict - resource already exists or invalid state transition.
    #[error("conflict: {0}")]
    Conflict(String),

    /// Insufficient credits.
    #[error("insufficient credits: balance={balance}, required={required}")]
    InsufficientCredits {
        /// Current balance.
        balance: i64,
        /// Required amount.
        required: i64,
    },

    /// Duplicate event (idempotency).
    #[error("duplicate event: {0}")]
    DuplicateEvent(String),

    /// Internal server error.
    #[error("internal error: {0}")]
    Internal(String),

    /// External service error.
    #[error("external service error: {0}")]
    ExternalService(String),
}

/// JSON error response body.
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match &self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                self.to_string(),
                None,
            ),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", self.to_string(), None),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone(), None),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone(), None),
            Self::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone(), None),
            Self::InsufficientCredits { balance, required } => (
                StatusCode::PAYMENT_REQUIRED,
                "insufficient_credits",
                self.to_string(),
                Some(serde_json::json!({
                    "balance": balance,
                    "required": required
                })),
            ),
            Self::DuplicateEvent(id) => (
                StatusCode::CONFLICT,
                "duplicate_event",
                format!("Event {id} already processed"),
                None,
            ),
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "Internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "An internal error occurred".to_string(),
                    None,
                )
            }
            Self::ExternalService(msg) => (
                StatusCode::BAD_GATEWAY,
                "external_service_error",
                msg.clone(),
                None,
            ),
        };

        let body = ErrorResponse {
            error: ErrorBody {
                code: code.to_string(),
                message,
                details,
            },
        };

        (status, Json(body)).into_response()
    }
}

impl From<z_billing_store::StoreError> for ApiError {
    fn from(err: z_billing_store::StoreError) -> Self {
        match err {
            z_billing_store::StoreError::NotFound { entity, id } => {
                Self::NotFound(format!("{entity} not found: {id}"))
            }
            z_billing_store::StoreError::InsufficientCredits { balance, required } => {
                Self::InsufficientCredits { balance, required }
            }
            z_billing_store::StoreError::DuplicateEvent { event_id } => {
                Self::DuplicateEvent(event_id)
            }
            z_billing_store::StoreError::Database(msg)
            | z_billing_store::StoreError::Serialization(msg) => Self::Internal(msg),
        }
    }
}
