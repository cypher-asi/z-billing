//! Stripe integration for payments and customer management.
//!
//! Stripe handles:
//! - Customer registration
//! - Credit purchases via Checkout
//! - Webhook handling for payment events
//! - Payment history

pub mod client;
pub mod types;

pub use client::StripeClient;
pub use client::StripeError;
pub use types::*;
