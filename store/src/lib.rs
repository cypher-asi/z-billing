//! `RocksDB` storage layer for z-billing.
//!
//! This crate provides persistent storage for accounts, transactions, and usage events
//! using `RocksDB` with column families for efficient indexing.
//!
//! # Architecture
//!
//! The storage uses the following column families:
//!
//! - `accounts`: Primary account records, keyed by `user_id`
//! - `transactions`: Credit transactions, keyed by `transaction_id` (ULID)
//! - `transactions_by_user`: Index for listing transactions by user
//! - `usage_events`: Usage events for idempotency checking, keyed by `event_id`
//!
//! # Example
//!
//! ```no_run
//! use z_billing_store::{RocksStore, Store};
//! use z_billing_core::{UserId, Account};
//!
//! let store = RocksStore::open("/tmp/z-billing-db").unwrap();
//!
//! // Create an account
//! let user_id = UserId::generate();
//! let account = Account::new(user_id);
//! store.put_account(&account).unwrap();
//!
//! // Get balance
//! let retrieved = store.get_account(&user_id).unwrap();
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

pub mod error;
pub mod keys;
pub mod rocks;
pub mod schema;

pub use error::{Result, StoreError};
pub use rocks::RocksStore;

use z_billing_core::{Account, CreditTransaction, TransactionId, UsageEvent, UserId};

/// The storage trait defining all database operations.
///
/// This trait abstracts the storage layer, allowing for different implementations
/// (e.g., `RocksDB`, in-memory for testing).
pub trait Store: Send + Sync {
    // =========================================================================
    // Account Operations
    // =========================================================================

    /// Insert or update an account record.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    fn put_account(&self, account: &Account) -> Result<()>;

    /// Get an account by user ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    fn get_account(&self, user_id: &UserId) -> Result<Option<Account>>;

    /// Delete an account by user ID.
    ///
    /// # Errors
    ///
    /// Returns `StoreError::NotFound` if the account doesn't exist.
    fn delete_account(&self, user_id: &UserId) -> Result<()>;

    /// Update account balance atomically.
    ///
    /// Returns the new balance after the update.
    ///
    /// # Errors
    ///
    /// Returns `StoreError::NotFound` if the account doesn't exist.
    fn update_balance(&self, user_id: &UserId, delta_cents: i64) -> Result<i64>;

    // =========================================================================
    // Transaction Operations
    // =========================================================================

    /// Insert a credit transaction.
    ///
    /// This also maintains the user index.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    fn put_transaction(&self, transaction: &CreditTransaction) -> Result<()>;

    /// Get a transaction by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    fn get_transaction(&self, transaction_id: &TransactionId) -> Result<Option<CreditTransaction>>;

    /// List transactions for a user, ordered by time (newest first).
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    fn list_transactions_by_user(
        &self,
        user_id: &UserId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<CreditTransaction>>;

    // =========================================================================
    // Usage Event Operations (for idempotency)
    // =========================================================================

    /// Check if a usage event has already been processed.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    fn has_usage_event(&self, event_id: &str) -> Result<bool>;

    /// Record a usage event for idempotency.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    fn put_usage_event(&self, event: &UsageEvent) -> Result<()>;

    /// Get a usage event by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    fn get_usage_event(&self, event_id: &str) -> Result<Option<UsageEvent>>;

    // =========================================================================
    // Compound Operations
    // =========================================================================

    /// Process a usage event: deduct credits and record transaction atomically.
    ///
    /// Returns the new balance after deduction.
    ///
    /// # Errors
    ///
    /// - `StoreError::NotFound` if the account doesn't exist.
    /// - `StoreError::InsufficientCredits` if balance is too low.
    /// - `StoreError::DuplicateEvent` if the event was already processed.
    fn process_usage(&self, event: &UsageEvent, transaction: &CreditTransaction) -> Result<i64>;

    /// Add credits to an account and record transaction atomically.
    ///
    /// Returns the new balance after addition.
    ///
    /// # Errors
    ///
    /// - `StoreError::NotFound` if the account doesn't exist.
    fn add_credits(
        &self,
        user_id: &UserId,
        amount_cents: i64,
        transaction: &CreditTransaction,
    ) -> Result<i64>;
}
