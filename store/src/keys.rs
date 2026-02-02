//! Key encoding utilities for `RocksDB`.
//!
//! This module provides functions for encoding and decoding keys used in column families.

use z_billing_core::{TransactionId, UserId};

/// Create an account key from a user ID.
#[must_use]
pub fn account_key(user_id: &UserId) -> Vec<u8> {
    user_id.as_bytes().to_vec()
}

/// Create a transaction key from a transaction ID.
#[must_use]
pub fn transaction_key(transaction_id: &TransactionId) -> Vec<u8> {
    transaction_id.to_bytes().to_vec()
}

/// Create a user-transaction index key.
///
/// Format: `user_id (16 bytes) || transaction_id (16 bytes)`
///
/// Since ULIDs are time-ordered, transactions for a user will be sorted by time.
#[must_use]
pub fn user_transaction_key(user_id: &UserId, transaction_id: &TransactionId) -> Vec<u8> {
    let mut key = Vec::with_capacity(32);
    key.extend_from_slice(user_id.as_bytes());
    key.extend_from_slice(&transaction_id.to_bytes());
    key
}

/// Create a prefix for iterating all transactions for a user.
#[must_use]
pub fn user_transactions_prefix(user_id: &UserId) -> Vec<u8> {
    user_id.as_bytes().to_vec()
}

/// Extract the transaction ID from a user-transaction index key.
///
/// # Panics
///
/// Panics if the key is not at least 32 bytes.
#[must_use]
pub fn extract_transaction_id_from_user_key(key: &[u8]) -> TransactionId {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&key[16..32]);
    TransactionId::from_bytes(bytes).expect("valid ULID bytes")
}

/// Create a usage event key from an event ID.
#[must_use]
pub fn usage_event_key(event_id: &str) -> Vec<u8> {
    event_id.as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_key_length() {
        let user_id = UserId::generate();
        let key = account_key(&user_id);
        assert_eq!(key.len(), 16);
    }

    #[test]
    fn transaction_key_length() {
        let tx_id = TransactionId::generate();
        let key = transaction_key(&tx_id);
        assert_eq!(key.len(), 16);
    }

    #[test]
    fn user_transaction_key_format() {
        let user_id = UserId::generate();
        let tx_id = TransactionId::generate();
        let key = user_transaction_key(&user_id, &tx_id);

        assert_eq!(key.len(), 32);
        assert_eq!(&key[..16], user_id.as_bytes());
        assert_eq!(&key[16..], tx_id.to_bytes());
    }

    #[test]
    fn extract_transaction_id_roundtrip() {
        let user_id = UserId::generate();
        let tx_id = TransactionId::generate();
        let key = user_transaction_key(&user_id, &tx_id);

        let extracted = extract_transaction_id_from_user_key(&key);
        assert_eq!(extracted, tx_id);
    }
}
