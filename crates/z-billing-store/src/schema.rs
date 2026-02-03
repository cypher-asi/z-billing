//! Database schema definitions and column families.
//!
//! This module defines the column families used in `RocksDB` storage.

/// Column family names for the `RocksDB` database.
pub mod cf {
    /// Primary account records, keyed by `user_id`.
    pub const ACCOUNTS: &str = "accounts";

    /// Credit transactions, keyed by `transaction_id` (ULID).
    pub const TRANSACTIONS: &str = "transactions";

    /// Index: transactions by user, keyed by `user_id || transaction_id`.
    /// Value is empty (index only).
    pub const TRANSACTIONS_BY_USER: &str = "transactions_by_user";

    /// Usage events for idempotency, keyed by `event_id`.
    pub const USAGE_EVENTS: &str = "usage_events";
}

/// Returns all column family names for database initialization.
#[must_use]
pub fn all_column_families() -> Vec<&'static str> {
    vec![
        cf::ACCOUNTS,
        cf::TRANSACTIONS,
        cf::TRANSACTIONS_BY_USER,
        cf::USAGE_EVENTS,
    ]
}
