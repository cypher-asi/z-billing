//! Core types and utilities for z-billing.
//!
//! This crate provides the foundational types used throughout the z-billing platform:
//!
//! - **Identifiers**: `UserId`, `TransactionId`, `AgentId`
//! - **Accounts**: `Account`, `Subscription`, `AutoRefill`
//! - **Credits**: `CreditTransaction`, `TransactionType`
//! - **Usage**: `UsageEvent`, `UsageSource`, `UsageMetric`
//! - **Pricing**: `PricingConfig`, `LlmPricing`
//!
//! # Z Credit Unit
//!
//! **1 Z Credit = $0.01 (1 cent)**
//!
//! - User buys $50 → Gets 5000 Z Credits
//! - LLM call costs 3 cents → Deducts 3 Z Credits
//! - Stored as `i64` (integer cents) to avoid floating point precision issues

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

pub mod account;
pub mod credits;
pub mod error;
pub mod ids;
pub mod pricing;
pub mod usage;

pub use account::{
    Account, AutoRefill, Plan, Subscription, SubscriptionStatus,
    DEFAULT_AUTO_REFILL_AMOUNT_CENTS, DEFAULT_AUTO_REFILL_TRIGGER_CENTS,
    PRO_PLAN_CREDITS, PRO_PLAN_DISCOUNT_PERCENT, PRO_PLAN_PRICE_CENTS,
    STANDARD_PLAN_CREDITS, STANDARD_PLAN_DISCOUNT_PERCENT, STANDARD_PLAN_PRICE_CENTS,
};
pub use credits::{CreditTransaction, TransactionType};
pub use error::{BillingError, Result};
pub use ids::{AgentId, IdError, TransactionId, UserId};
pub use pricing::{LlmPricing, ModelKey, PricingConfig};
pub use usage::{LlmProvider, TokenDirection, UsageEvent, UsageMetric, UsageSource};
