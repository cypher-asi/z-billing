# Z-Billing v0.1.0 - Credit Transactions

This document specifies the credit transaction system for tracking all balance changes.

## Overview

All changes to an account's balance are recorded as credit transactions. Transactions form an immutable ledger that provides:

- **Audit trail**: Complete history of all balance changes
- **Accountability**: Each transaction has a type and description
- **Balance verification**: Running balance tracked with each transaction

## CreditTransaction

A credit transaction representing a balance change.

### Structure

```rust
pub struct CreditTransaction {
    /// Unique transaction ID (ULID for time-ordering).
    pub id: TransactionId,

    /// The user whose balance was affected.
    pub user_id: UserId,

    /// Amount in cents. Positive = credit, Negative = debit.
    pub amount_cents: i64,

    /// Type of transaction.
    pub transaction_type: TransactionType,

    /// Balance after this transaction (in cents).
    pub balance_after_cents: i64,

    /// Human-readable description.
    pub description: String,

    /// Additional metadata (source, agent_id, model, etc.).
    pub metadata: serde_json::Value,

    /// When the transaction was created.
    pub created_at: DateTime<Utc>,
}
```

### JSON Example

```json
{
  "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "amount_cents": -300,
  "transaction_type": "usage",
  "balance_after_cents": 4700,
  "description": "LLM usage: anthropic claude-3-5-sonnet (500 input, 1000 output tokens) via aura-runtime",
  "metadata": {
    "model": "claude-3-5-sonnet",
    "provider": "anthropic",
    "input_tokens": 500,
    "output_tokens": 1000
  },
  "created_at": "2025-01-15T10:30:00Z"
}
```

## TransactionType

The type of balance change.

### Definition

```rust
pub enum TransactionType {
    Purchase,           // User purchased credits
    Usage,              // Credits deducted for usage
    SubscriptionGrant,  // Monthly subscription credit grant
    Refund,             // Refund issued
    Bonus,              // Promotional/bonus credits
    AutoRefill,         // Automatic refill triggered
}
```

### Classification

| Type              | Direction | Amount Sign | Description                           |
|-------------------|-----------|-------------|---------------------------------------|
| `Purchase`        | Credit    | Positive    | User buys credits via Stripe          |
| `SubscriptionGrant` | Credit  | Positive    | Monthly allowance from subscription   |
| `Refund`          | Credit    | Positive    | Refund for failed service, etc.       |
| `Bonus`           | Credit    | Positive    | Promotional credits                   |
| `AutoRefill`      | Credit    | Positive    | Automatic purchase when balance low   |
| `Usage`           | Debit     | Negative    | Service usage deduction               |

### Methods

```rust
impl TransactionType {
    /// Check if this transaction type adds credits (positive balance change).
    pub const fn is_credit(&self) -> bool {
        matches!(self,
            Self::Purchase |
            Self::SubscriptionGrant |
            Self::Refund |
            Self::Bonus |
            Self::AutoRefill
        )
    }

    /// Check if this transaction type removes credits (negative balance change).
    pub const fn is_debit(&self) -> bool {
        matches!(self, Self::Usage)
    }
}
```

## Transaction Creation

Transactions are created via factory methods that ensure consistent formatting.

### Purchase

```rust
CreditTransaction::purchase(
    user_id,
    amount_cents: 5000,      // Credits being added
    balance_after_cents: 5000,
    "Purchased $50.00 credits via Stripe".into(),
)
```

### Usage

```rust
CreditTransaction::usage(
    user_id,
    amount_cents: 100,       // Will be negated internally
    balance_after_cents: 4900,
    "LLM usage".into(),
    serde_json::json!({"model": "claude-3-5-sonnet"}),
)
// Results in amount_cents: -100 (negative)
```

### Subscription Grant

```rust
CreditTransaction::subscription_grant(
    user_id,
    amount_cents: 2500,      // Monthly Standard plan grant
    balance_after_cents: 7500,
    "Standard",              // Plan name
)
// Description: "Monthly Standard plan credit grant"
// Metadata: {"plan": "Standard"}
```

### Refund

```rust
CreditTransaction::refund(
    user_id,
    amount_cents: 50,
    balance_after_cents: 4950,
    "Refund for failed API call".into(),
)
```

### Bonus

```rust
CreditTransaction::bonus(
    user_id,
    amount_cents: 500,
    balance_after_cents: 5500,
    "Welcome bonus for new account".into(),
)
```

### Auto-Refill

```rust
CreditTransaction::auto_refill(
    user_id,
    amount_cents: 2500,
    balance_after_cents: 3000,
)
// Description: "Auto-refill of 2500 credits"
```

## Transaction Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Transaction Creation Flows                          │
└─────────────────────────────────────────────────────────────────────────────┘

                       ┌──────────────────────┐
                       │       Purchase       │
                       └──────────┬───────────┘
                                  │
  ┌───────────────────────────────┼───────────────────────────────┐
  │                               │                               │
  ▼                               ▼                               ▼
┌─────────────┐           ┌──────────────┐            ┌───────────────┐
│   Stripe    │           │ Subscription │            │  Auto-Refill  │
│  Checkout   │           │    Grant     │            │    Trigger    │
│  Completed  │           │  (Monthly)   │            │               │
└──────┬──────┘           └──────┬───────┘            └───────┬───────┘
       │                         │                            │
       └─────────────────────────┴────────────────────────────┘
                                  │
                                  ▼
                       ┌──────────────────────┐
                       │   add_credits()      │
                       │   - Update balance   │
                       │   - Create TX record │
                       │   - Atomic operation │
                       └──────────────────────┘


                       ┌──────────────────────┐
                       │        Usage         │
                       └──────────┬───────────┘
                                  │
  ┌───────────────────────────────┼───────────────────────────────┐
  │                               │                               │
  ▼                               ▼                               ▼
┌─────────────┐           ┌──────────────┐            ┌───────────────┐
│ LLM Tokens  │           │   Compute    │            │   API Calls   │
│             │           │  (CPU/Mem)   │            │               │
└──────┬──────┘           └──────┬───────┘            └───────┬───────┘
       │                         │                            │
       └─────────────────────────┴────────────────────────────┘
                                  │
                                  ▼
                       ┌──────────────────────┐
                       │   process_usage()    │
                       │   - Check balance    │
                       │   - Deduct credits   │
                       │   - Create TX record │
                       │   - Store event (idem│
                       │   - Atomic operation │
                       └──────────────────────┘
```

## Idempotency

Usage transactions include an `event_id` in the associated usage event for idempotency:

- Each usage event must have a unique `event_id`
- If the same `event_id` is submitted twice, the second request is rejected
- This prevents double-charging from network retries

## Balance Consistency

The `balance_after_cents` field provides a running balance that should satisfy:

```
transaction[n].balance_after_cents = 
    transaction[n-1].balance_after_cents + transaction[n].amount_cents
```

This allows verification of ledger integrity and debugging of balance discrepancies.

## Storage

Transactions are stored in two column families:

| Column Family          | Key Format                      | Value              |
|------------------------|---------------------------------|--------------------|
| `transactions`         | `transaction_id` (16 bytes)     | Transaction (CBOR) |
| `transactions_by_user` | `user_id` + `transaction_id`    | Empty (index)      |

The `transactions_by_user` index enables efficient queries for a user's transaction history, sorted by time (newest first due to ULID ordering).
