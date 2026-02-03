//! Lago integration for usage reporting and subscription management.
//!
//! Lago handles:
//! - Usage event aggregation for analytics
//! - Subscription management (plans, billing cycles)
//! - Invoice generation
//! - Customer organization

pub mod client;
pub mod types;

pub use client::LagoClient;
pub use types::*;
