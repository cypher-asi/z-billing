//! Z-Billing Client SDK.
//!
//! This crate provides a client library for services to interact with the z-billing API.
//!
//! # Example
//!
//! ```no_run
//! use z_billing_client::{ZBillingClient, LlmUsageEvent};
//!
//! # async fn example() -> Result<(), z_billing_client::ClientError> {
//! let client = ZBillingClient::new(
//!     "http://z-billing.billing-system.svc:8080",
//!     "your-service-api-key",
//! )?;
//!
//! // Report LLM usage
//! let response = client.report_llm_usage(LlmUsageEvent {
//!     event_id: "evt_123".to_string(),
//!     user_id: "user-uuid".to_string(),
//!     agent_id: Some("agent-uuid".to_string()),
//!     provider: "anthropic".to_string(),
//!     model: "claude-3-5-sonnet".to_string(),
//!     input_tokens: 1000,
//!     output_tokens: 500,
//!     metadata: None,
//! }).await?;
//!
//! println!("New balance: {} credits", response.balance_cents);
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

mod client;
mod error;
mod types;

pub use client::{ClientOptions, ZBillingClient};
pub use error::ClientError;
pub use types::*;
