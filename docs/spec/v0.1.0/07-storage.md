# Z-Billing v0.1.0 - Storage Layer

This document specifies the storage layer using RocksDB for persistent data.

## Overview

The `z-billing-store` crate provides:

- **`Store` trait**: Abstract storage interface
- **`RocksStore`**: RocksDB implementation with column families
- **Atomic operations**: Compound operations for consistency

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              z-billing-store                                │
└─────────────────────────────────────────────────────────────────────────────┘

  ┌──────────────────┐              ┌──────────────────┐
  │   Store Trait    │◀─────────────│   RocksStore     │
  │                  │ implements   │   (RocksDB)      │
  └────────┬─────────┘              └──────────────────┘
           │
           │ defines interface for
           ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │                        Column Families                          │
  ├─────────────────────────────────────────────────────────────────┤
  │                                                                 │
  │  ┌─────────────────┐    ┌─────────────────────────────────────┐│
  │  │    accounts     │    │ Primary account records              ││
  │  │                 │    │ Key: user_id (16 bytes)             ││
  │  └─────────────────┘    └─────────────────────────────────────┘│
  │                                                                 │
  │  ┌─────────────────┐    ┌─────────────────────────────────────┐│
  │  │  transactions   │    │ Credit transaction records           ││
  │  │                 │    │ Key: transaction_id (16 bytes)      ││
  │  └─────────────────┘    └─────────────────────────────────────┘│
  │                                                                 │
  │  ┌─────────────────┐    ┌─────────────────────────────────────┐│
  │  │ transactions_by │    │ Index for user transaction queries   ││
  │  │     _user       │    │ Key: user_id + transaction_id       ││
  │  └─────────────────┘    └─────────────────────────────────────┘│
  │                                                                 │
  │  ┌─────────────────┐    ┌─────────────────────────────────────┐│
  │  │  usage_events   │    │ Events for idempotency checking      ││
  │  │                 │    │ Key: event_id (string bytes)        ││
  │  └─────────────────┘    └─────────────────────────────────────┘│
  │                                                                 │
  └─────────────────────────────────────────────────────────────────┘
```

## Column Families

### Schema Definition

```rust
pub mod cf {
    /// Primary account records, keyed by user_id.
    pub const ACCOUNTS: &str = "accounts";

    /// Credit transactions, keyed by transaction_id (ULID).
    pub const TRANSACTIONS: &str = "transactions";

    /// Index: transactions by user, keyed by user_id || transaction_id.
    pub const TRANSACTIONS_BY_USER: &str = "transactions_by_user";

    /// Usage events for idempotency, keyed by event_id.
    pub const USAGE_EVENTS: &str = "usage_events";
}
```

### Column Family Details

| Column Family          | Key                           | Value           | Purpose                    |
|------------------------|-------------------------------|-----------------|----------------------------|
| `accounts`             | `user_id` (16 bytes)          | Account (CBOR)  | Primary account storage    |
| `transactions`         | `transaction_id` (16 bytes)   | Transaction (CBOR) | Transaction storage     |
| `transactions_by_user` | `user_id` + `transaction_id` (32 bytes) | Empty | User transaction index |
| `usage_events`         | `event_id` (string bytes)     | UsageEvent (CBOR) | Idempotency checking    |

## Key Encoding

### Account Key

```rust
pub fn account_key(user_id: &UserId) -> Vec<u8> {
    user_id.as_bytes().to_vec()  // 16 bytes (UUID)
}
```

### Transaction Key

```rust
pub fn transaction_key(transaction_id: &TransactionId) -> Vec<u8> {
    transaction_id.to_bytes().to_vec()  // 16 bytes (ULID)
}
```

### User-Transaction Index Key

```rust
/// Format: user_id (16 bytes) || transaction_id (16 bytes)
pub fn user_transaction_key(user_id: &UserId, transaction_id: &TransactionId) -> Vec<u8> {
    let mut key = Vec::with_capacity(32);
    key.extend_from_slice(user_id.as_bytes());
    key.extend_from_slice(&transaction_id.to_bytes());
    key
}
```

The ULID timestamp prefix in `transaction_id` ensures transactions are naturally sorted by time within each user's range.

### Usage Event Key

```rust
pub fn usage_event_key(event_id: &str) -> Vec<u8> {
    event_id.as_bytes().to_vec()
}
```

## Store Trait

### Interface Definition

```rust
pub trait Store: Send + Sync {
    // Account Operations
    fn put_account(&self, account: &Account) -> Result<()>;
    fn get_account(&self, user_id: &UserId) -> Result<Option<Account>>;
    fn delete_account(&self, user_id: &UserId) -> Result<()>;
    fn update_balance(&self, user_id: &UserId, delta_cents: i64) -> Result<i64>;

    // Transaction Operations
    fn put_transaction(&self, transaction: &CreditTransaction) -> Result<()>;
    fn get_transaction(&self, transaction_id: &TransactionId) -> Result<Option<CreditTransaction>>;
    fn list_transactions_by_user(
        &self,
        user_id: &UserId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<CreditTransaction>>;

    // Usage Event Operations
    fn has_usage_event(&self, event_id: &str) -> Result<bool>;
    fn put_usage_event(&self, event: &UsageEvent) -> Result<()>;
    fn get_usage_event(&self, event_id: &str) -> Result<Option<UsageEvent>>;

    // Compound Operations (atomic)
    fn process_usage(&self, event: &UsageEvent, transaction: &CreditTransaction) -> Result<i64>;
    fn add_credits(
        &self,
        user_id: &UserId,
        amount_cents: i64,
        transaction: &CreditTransaction,
    ) -> Result<i64>;
}
```

## Compound Operations

### process_usage

Atomically processes a usage event:

1. Check if event already processed (idempotency)
2. Verify account has sufficient balance
3. Deduct credits from account
4. Store the credit transaction
5. Store the usage event

```
process_usage(event, transaction)
│
├── Check has_usage_event(event.event_id)
│   └── If exists → return DuplicateEvent error
│
├── Get account(event.user_id)
│   └── If not found → return NotFound error
│
├── Check balance >= transaction.amount_cents
│   └── If insufficient → return InsufficientCredits error
│
└── Atomic batch write:
    ├── Update account balance
    ├── Store transaction
    ├── Update transactions_by_user index
    └── Store usage event
```

### add_credits

Atomically adds credits to an account:

1. Get current account
2. Update balance
3. Store the credit transaction

```
add_credits(user_id, amount_cents, transaction)
│
├── Get account(user_id)
│   └── If not found → return NotFound error
│
└── Atomic batch write:
    ├── Update account balance
    ├── Update lifetime stats
    ├── Store transaction
    └── Update transactions_by_user index
```

## Error Types

```rust
pub enum StoreError {
    /// Resource not found.
    NotFound,

    /// Insufficient credits for operation.
    InsufficientCredits { balance: i64, required: i64 },

    /// Duplicate event (idempotency violation).
    DuplicateEvent { event_id: String },

    /// Database error.
    Database(String),

    /// Serialization error.
    Serialization(String),
}
```

## RocksStore Implementation

### Initialization

```rust
let store = RocksStore::open("/data/z-billing")?;
```

The `open` function:
1. Creates the data directory if it doesn't exist
2. Opens RocksDB with all column families
3. Returns a thread-safe `RocksStore` instance

### Serialization

All values are serialized using CBOR (Concise Binary Object Representation):

- Compact binary format
- Schema-flexible (forward/backward compatible)
- Fast encode/decode

### Thread Safety

`RocksStore` implements `Send + Sync` and can be safely shared across threads via `Arc<RocksStore>`.

## Usage Example

```rust
use z_billing_store::{RocksStore, Store};
use z_billing_core::{UserId, Account, CreditTransaction};

// Open database
let store = RocksStore::open("/data/z-billing")?;

// Create an account
let user_id = UserId::generate();
let account = Account::new(user_id);
store.put_account(&account)?;

// Add credits
let tx = CreditTransaction::purchase(
    user_id,
    5000,  // credits
    5000,  // balance after
    "Initial purchase".into(),
);
store.add_credits(&user_id, 5000, &tx)?;

// Get balance
let account = store.get_account(&user_id)?.unwrap();
println!("Balance: {} credits", account.balance_cents);

// List transactions
let transactions = store.list_transactions_by_user(&user_id, 10, 0)?;
```

## Transaction Listing

The `transactions_by_user` index enables efficient pagination:

```rust
fn list_transactions_by_user(
    &self,
    user_id: &UserId,
    limit: usize,
    offset: usize,
) -> Result<Vec<CreditTransaction>> {
    // Create iterator starting at user's prefix
    let prefix = user_transactions_prefix(user_id);
    
    // Iterate in reverse (newest first, due to ULID ordering)
    // Skip 'offset' entries, take 'limit' entries
    // For each index entry, look up full transaction from 'transactions' CF
}
```

## Data Durability

RocksDB provides:

- **Write-ahead logging (WAL)**: Durability before acknowledgment
- **Snapshots**: Consistent reads during iteration
- **Atomic batches**: Multiple writes as single transaction
- **Compaction**: Background cleanup of deleted data
