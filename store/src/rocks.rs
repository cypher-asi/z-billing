//! `RocksDB` storage implementation.
//!
//! This module provides the `RocksStore` implementation of the `Store` trait.

use std::path::Path;
use std::sync::Arc;

use rocksdb::{
    BoundColumnFamily, ColumnFamilyDescriptor, DBWithThreadMode, IteratorMode, MultiThreaded,
    Options, WriteBatch,
};

use z_billing_core::{Account, CreditTransaction, TransactionId, UsageEvent, UserId};

use crate::error::{Result, StoreError};
use crate::keys;
use crate::schema::{all_column_families, cf};
use crate::Store;

/// RocksDB-backed storage implementation.
pub struct RocksStore {
    db: Arc<DBWithThreadMode<MultiThreaded>>,
}

impl RocksStore {
    /// Open or create a `RocksDB` database at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or created.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cf_descriptors: Vec<_> = all_column_families()
            .into_iter()
            .map(|name| ColumnFamilyDescriptor::new(name, Options::default()))
            .collect();

        let db = DBWithThreadMode::open_cf_descriptors(&opts, path, cf_descriptors)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Get a column family handle.
    fn cf(&self, name: &str) -> Result<Arc<BoundColumnFamily<'_>>> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| StoreError::Database(format!("column family not found: {name}")))
    }

    /// Serialize a value using CBOR.
    fn serialize<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        ciborium::into_writer(value, &mut buf)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        Ok(buf)
    }

    /// Deserialize a value from CBOR.
    fn deserialize<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T> {
        ciborium::from_reader(data).map_err(|e| StoreError::Serialization(e.to_string()))
    }
}

impl Store for RocksStore {
    // =========================================================================
    // Account Operations
    // =========================================================================

    fn put_account(&self, account: &Account) -> Result<()> {
        let cf = self.cf(cf::ACCOUNTS)?;
        let key = keys::account_key(&account.user_id);
        let value = Self::serialize(account)?;

        self.db
            .put_cf(&cf, key, value)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    fn get_account(&self, user_id: &UserId) -> Result<Option<Account>> {
        let cf = self.cf(cf::ACCOUNTS)?;
        let key = keys::account_key(user_id);

        self.db
            .get_cf(&cf, key)
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map(|data| Self::deserialize(&data))
            .transpose()
    }

    fn delete_account(&self, user_id: &UserId) -> Result<()> {
        let cf = self.cf(cf::ACCOUNTS)?;
        let key = keys::account_key(user_id);

        // Check if account exists
        if self.get_account(user_id)?.is_none() {
            return Err(StoreError::NotFound);
        }

        self.db
            .delete_cf(&cf, key)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    fn update_balance(&self, user_id: &UserId, delta_cents: i64) -> Result<i64> {
        let cf = self.cf(cf::ACCOUNTS)?;
        let key = keys::account_key(user_id);

        // Get current account
        let mut account = self.get_account(user_id)?.ok_or(StoreError::NotFound)?;

        // Update balance
        account.balance_cents += delta_cents;
        account.updated_at = chrono::Utc::now();

        // Track lifetime stats
        if delta_cents > 0 {
            // This is a simplified approach; the actual type tracking happens in the service
        } else {
            account.lifetime_used_cents += delta_cents.abs();
        }

        let value = Self::serialize(&account)?;
        self.db
            .put_cf(&cf, key, value)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(account.balance_cents)
    }

    // =========================================================================
    // Transaction Operations
    // =========================================================================

    fn put_transaction(&self, transaction: &CreditTransaction) -> Result<()> {
        let cf_tx = self.cf(cf::TRANSACTIONS)?;
        let cf_by_user = self.cf(cf::TRANSACTIONS_BY_USER)?;

        let tx_key = keys::transaction_key(&transaction.id);
        let user_tx_key = keys::user_transaction_key(&transaction.user_id, &transaction.id);
        let value = Self::serialize(transaction)?;

        let mut batch = WriteBatch::default();
        batch.put_cf(&cf_tx, &tx_key, &value);
        batch.put_cf(&cf_by_user, &user_tx_key, []); // Index entry (empty value)

        self.db
            .write(batch)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    fn get_transaction(&self, transaction_id: &TransactionId) -> Result<Option<CreditTransaction>> {
        let cf = self.cf(cf::TRANSACTIONS)?;
        let key = keys::transaction_key(transaction_id);

        self.db
            .get_cf(&cf, key)
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map(|data| Self::deserialize(&data))
            .transpose()
    }

    fn list_transactions_by_user(
        &self,
        user_id: &UserId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<CreditTransaction>> {
        let cf_by_user = self.cf(cf::TRANSACTIONS_BY_USER)?;
        let prefix = keys::user_transactions_prefix(user_id);

        let mut transactions = Vec::new();
        let mut skipped = 0;

        // Iterate in reverse order (newest first) using the end of the prefix range
        let iter = self.db.iterator_cf(
            &cf_by_user,
            IteratorMode::From(&prefix, rocksdb::Direction::Forward),
        );

        // Collect all matching keys first (since ULIDs are naturally time-ordered)
        let mut all_keys: Vec<Vec<u8>> = Vec::new();
        for item in iter {
            let (key, _) = item.map_err(|e| StoreError::Database(e.to_string()))?;

            if !key.starts_with(&prefix) {
                break;
            }

            all_keys.push(key.to_vec());
        }

        // Reverse to get newest first
        all_keys.reverse();

        for key in all_keys {
            if skipped < offset {
                skipped += 1;
                continue;
            }

            if transactions.len() >= limit {
                break;
            }

            let tx_id = keys::extract_transaction_id_from_user_key(&key);
            if let Some(tx) = self.get_transaction(&tx_id)? {
                transactions.push(tx);
            }
        }

        Ok(transactions)
    }

    // =========================================================================
    // Usage Event Operations
    // =========================================================================

    fn has_usage_event(&self, event_id: &str) -> Result<bool> {
        let cf = self.cf(cf::USAGE_EVENTS)?;
        let key = keys::usage_event_key(event_id);

        let exists = self
            .db
            .get_cf(&cf, key)
            .map_err(|e| StoreError::Database(e.to_string()))?
            .is_some();

        Ok(exists)
    }

    fn put_usage_event(&self, event: &UsageEvent) -> Result<()> {
        let cf = self.cf(cf::USAGE_EVENTS)?;
        let key = keys::usage_event_key(&event.event_id);
        let value = Self::serialize(event)?;

        self.db
            .put_cf(&cf, key, value)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    fn get_usage_event(&self, event_id: &str) -> Result<Option<UsageEvent>> {
        let cf = self.cf(cf::USAGE_EVENTS)?;
        let key = keys::usage_event_key(event_id);

        self.db
            .get_cf(&cf, key)
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map(|data| Self::deserialize(&data))
            .transpose()
    }

    // =========================================================================
    // Compound Operations
    // =========================================================================

    fn process_usage(&self, event: &UsageEvent, transaction: &CreditTransaction) -> Result<i64> {
        // Check for duplicate event
        if self.has_usage_event(&event.event_id)? {
            return Err(StoreError::DuplicateEvent {
                event_id: event.event_id.clone(),
            });
        }

        // Get current account
        let mut account = self
            .get_account(&event.user_id)?
            .ok_or(StoreError::NotFound)?;

        // Check sufficient balance
        if account.balance_cents < event.cost_cents {
            return Err(StoreError::InsufficientCredits {
                balance: account.balance_cents,
                required: event.cost_cents,
            });
        }

        // Prepare updates
        let cf_accounts = self.cf(cf::ACCOUNTS)?;
        let cf_tx = self.cf(cf::TRANSACTIONS)?;
        let cf_tx_by_user = self.cf(cf::TRANSACTIONS_BY_USER)?;
        let cf_usage = self.cf(cf::USAGE_EVENTS)?;

        // Update account
        account.balance_cents -= event.cost_cents;
        account.lifetime_used_cents += event.cost_cents;
        account.updated_at = chrono::Utc::now();

        let account_key = keys::account_key(&event.user_id);
        let tx_key = keys::transaction_key(&transaction.id);
        let user_tx_key = keys::user_transaction_key(&event.user_id, &transaction.id);
        let event_key = keys::usage_event_key(&event.event_id);

        let account_value = Self::serialize(&account)?;
        let tx_value = Self::serialize(transaction)?;
        let event_value = Self::serialize(event)?;

        // Write atomically
        let mut batch = WriteBatch::default();
        batch.put_cf(&cf_accounts, &account_key, &account_value);
        batch.put_cf(&cf_tx, &tx_key, &tx_value);
        batch.put_cf(&cf_tx_by_user, &user_tx_key, []);
        batch.put_cf(&cf_usage, &event_key, &event_value);

        self.db
            .write(batch)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(account.balance_cents)
    }

    fn add_credits(
        &self,
        user_id: &UserId,
        amount_cents: i64,
        transaction: &CreditTransaction,
    ) -> Result<i64> {
        // Get current account
        let mut account = self.get_account(user_id)?.ok_or(StoreError::NotFound)?;

        // Prepare updates
        let cf_accounts = self.cf(cf::ACCOUNTS)?;
        let cf_tx = self.cf(cf::TRANSACTIONS)?;
        let cf_tx_by_user = self.cf(cf::TRANSACTIONS_BY_USER)?;

        // Update account
        account.balance_cents += amount_cents;
        account.updated_at = chrono::Utc::now();

        // Track lifetime stats based on transaction type
        match transaction.transaction_type {
            z_billing_core::TransactionType::Purchase
            | z_billing_core::TransactionType::AutoRefill => {
                account.lifetime_purchased_cents += amount_cents;
            }
            z_billing_core::TransactionType::SubscriptionGrant
            | z_billing_core::TransactionType::Bonus => {
                account.lifetime_granted_cents += amount_cents;
            }
            _ => {}
        }

        let account_key = keys::account_key(user_id);
        let tx_key = keys::transaction_key(&transaction.id);
        let user_tx_key = keys::user_transaction_key(user_id, &transaction.id);

        let account_value = Self::serialize(&account)?;
        let tx_value = Self::serialize(transaction)?;

        // Write atomically
        let mut batch = WriteBatch::default();
        batch.put_cf(&cf_accounts, &account_key, &account_value);
        batch.put_cf(&cf_tx, &tx_key, &tx_value);
        batch.put_cf(&cf_tx_by_user, &user_tx_key, []);

        self.db
            .write(batch)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(account.balance_cents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use z_billing_core::{CreditTransaction, UsageEvent, UsageMetric, UsageSource};

    fn create_test_store() -> (RocksStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = RocksStore::open(dir.path()).unwrap();
        (store, dir)
    }

    #[test]
    fn account_crud() {
        let (store, _dir) = create_test_store();
        let user_id = UserId::generate();
        let mut account = Account::new(user_id);
        account.balance_cents = 5000;

        // Create
        store.put_account(&account).unwrap();

        // Read
        let retrieved = store.get_account(&user_id).unwrap().unwrap();
        assert_eq!(retrieved.balance_cents, 5000);

        // Update balance
        let new_balance = store.update_balance(&user_id, -100).unwrap();
        assert_eq!(new_balance, 4900);

        let updated = store.get_account(&user_id).unwrap().unwrap();
        assert_eq!(updated.balance_cents, 4900);
        assert_eq!(updated.lifetime_used_cents, 100);

        // Delete
        store.delete_account(&user_id).unwrap();
        assert!(store.get_account(&user_id).unwrap().is_none());
    }

    #[test]
    fn transaction_operations() {
        let (store, _dir) = create_test_store();
        let user_id = UserId::generate();

        // Create account first
        let account = Account::new(user_id);
        store.put_account(&account).unwrap();

        // Create and store transactions with a delay to ensure different ULID timestamps
        // (ULIDs are generated at creation time, not storage time)
        let tx1 = CreditTransaction::purchase(user_id, 5000, 5000, "Purchase 1".into());
        store.put_transaction(&tx1).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(2)); // Ensure different ULIDs

        let tx2 = CreditTransaction::purchase(user_id, 2500, 7500, "Purchase 2".into());
        store.put_transaction(&tx2).unwrap();

        // Get single transaction
        let retrieved = store.get_transaction(&tx1.id).unwrap().unwrap();
        assert_eq!(retrieved.amount_cents, 5000);

        // List transactions (newest first)
        let transactions = store.list_transactions_by_user(&user_id, 10, 0).unwrap();
        assert_eq!(transactions.len(), 2);
        assert_eq!(transactions[0].description, "Purchase 2"); // Newest first
        assert_eq!(transactions[1].description, "Purchase 1");

        // Pagination
        let page1 = store.list_transactions_by_user(&user_id, 1, 0).unwrap();
        let page2 = store.list_transactions_by_user(&user_id, 1, 1).unwrap();
        assert_eq!(page1.len(), 1);
        assert_eq!(page2.len(), 1);
        assert_eq!(page1[0].description, "Purchase 2");
        assert_eq!(page2[0].description, "Purchase 1");
    }

    #[test]
    fn usage_event_idempotency() {
        let (store, _dir) = create_test_store();
        let user_id = UserId::generate();

        // Create account with balance
        let mut account = Account::new(user_id);
        account.balance_cents = 1000;
        store.put_account(&account).unwrap();

        let event = UsageEvent {
            event_id: "evt_123".to_string(),
            user_id,
            agent_id: None,
            source: UsageSource::AuraRuntime,
            metric: UsageMetric::ApiCalls {
                endpoint: "test".to_string(),
            },
            quantity: 1.0,
            cost_cents: 10,
            timestamp: chrono::Utc::now(),
            metadata: serde_json::Value::Null,
        };

        let tx =
            CreditTransaction::usage(user_id, 10, 990, "API call".into(), serde_json::json!({}));

        // First call should succeed
        let balance = store.process_usage(&event, &tx).unwrap();
        assert_eq!(balance, 990);

        // Second call should fail with duplicate error
        let result = store.process_usage(&event, &tx);
        assert!(matches!(result, Err(StoreError::DuplicateEvent { .. })));
    }

    #[test]
    fn insufficient_credits() {
        let (store, _dir) = create_test_store();
        let user_id = UserId::generate();

        // Create account with low balance
        let mut account = Account::new(user_id);
        account.balance_cents = 5;
        store.put_account(&account).unwrap();

        let event = UsageEvent {
            event_id: "evt_456".to_string(),
            user_id,
            agent_id: None,
            source: UsageSource::AuraRuntime,
            metric: UsageMetric::ApiCalls {
                endpoint: "test".to_string(),
            },
            quantity: 1.0,
            cost_cents: 100, // More than balance
            timestamp: chrono::Utc::now(),
            metadata: serde_json::Value::Null,
        };

        let tx =
            CreditTransaction::usage(user_id, 100, 0, "API call".into(), serde_json::json!({}));

        let result = store.process_usage(&event, &tx);
        assert!(matches!(
            result,
            Err(StoreError::InsufficientCredits {
                balance: 5,
                required: 100
            })
        ));
    }

    #[test]
    fn add_credits_with_transaction() {
        let (store, _dir) = create_test_store();
        let user_id = UserId::generate();

        // Create account
        let account = Account::new(user_id);
        store.put_account(&account).unwrap();

        // Add credits
        let tx = CreditTransaction::purchase(user_id, 5000, 5000, "Purchase $50".into());
        let balance = store.add_credits(&user_id, 5000, &tx).unwrap();
        assert_eq!(balance, 5000);

        // Verify account updated
        let account = store.get_account(&user_id).unwrap().unwrap();
        assert_eq!(account.balance_cents, 5000);
        assert_eq!(account.lifetime_purchased_cents, 5000);

        // Verify transaction recorded
        let transactions = store.list_transactions_by_user(&user_id, 10, 0).unwrap();
        assert_eq!(transactions.len(), 1);
    }
}
