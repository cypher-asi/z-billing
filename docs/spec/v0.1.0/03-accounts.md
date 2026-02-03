# Z-Billing v0.1.0 - Account Domain

This document specifies the account structure, subscriptions, and plans.

## Account

A billing account for a user, tracking balance and subscription status.

### Structure

```rust
pub struct Account {
    /// The user ID (from Zero-ID)
    pub user_id: UserId,
    
    /// Current Z Credit balance in cents (1 Z Credit = $0.01)
    pub balance_cents: i64,
    
    /// Lifetime credits purchased (in cents)
    pub lifetime_purchased_cents: i64,
    
    /// Lifetime credits granted from subscriptions (in cents)
    pub lifetime_granted_cents: i64,
    
    /// Lifetime credits used (in cents)
    pub lifetime_used_cents: i64,
    
    /// Current subscription, if any
    pub subscription: Option<Subscription>,
    
    /// Auto-refill configuration, if enabled
    pub auto_refill: Option<AutoRefill>,
    
    /// Lago customer ID for usage reporting
    pub lago_customer_id: Option<String>,
    
    /// Stripe customer ID for payments
    pub stripe_customer_id: Option<String>,
    
    /// When the account was created
    pub created_at: DateTime<Utc>,
    
    /// When the account was last updated
    pub updated_at: DateTime<Utc>,
}
```

### JSON Example

```json
{
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "balance_cents": 4500,
  "lifetime_purchased_cents": 10000,
  "lifetime_granted_cents": 5000,
  "lifetime_used_cents": 10500,
  "subscription": {
    "plan": "standard",
    "status": "active",
    "current_period_start": "2025-01-01T00:00:00Z",
    "current_period_end": "2025-02-01T00:00:00Z",
    "lago_subscription_id": "sub_abc123",
    "created_at": "2024-06-15T10:30:00Z"
  },
  "auto_refill": {
    "enabled": true,
    "trigger_below_cents": 500,
    "refill_amount_cents": 2500
  },
  "lago_customer_id": "cus_lago_xyz",
  "stripe_customer_id": "cus_stripe_abc",
  "created_at": "2024-06-15T10:30:00Z",
  "updated_at": "2025-01-15T14:22:00Z"
}
```

### Balance Invariant

The following relationship should hold:

```
balance_cents = lifetime_purchased_cents 
              + lifetime_granted_cents 
              - lifetime_used_cents
              + adjustments (refunds, bonuses)
```

### Operations

```rust
// Create new account with zero balance
let account = Account::new(user_id);

// Check if user can afford a deduction
account.has_sufficient_credits(100) // true if balance >= 100

// Get current plan (Free if no subscription)
account.current_plan() // Plan::Free | Plan::Standard | Plan::Pro | Plan::Enterprise

// Check subscription status
account.has_active_subscription() // true if subscription exists and is Active
```

## Subscription

A subscription to a billing plan with recurring credit grants.

### Structure

```rust
pub struct Subscription {
    /// The subscription plan
    pub plan: Plan,
    
    /// Current status of the subscription
    pub status: SubscriptionStatus,
    
    /// Start of the current billing period
    pub current_period_start: DateTime<Utc>,
    
    /// End of the current billing period
    pub current_period_end: DateTime<Utc>,
    
    /// Lago subscription ID
    pub lago_subscription_id: String,
    
    /// When the subscription was created
    pub created_at: DateTime<Utc>,
}
```

## Subscription Status

```rust
pub enum SubscriptionStatus {
    Active,     // Subscription is active
    Cancelled,  // Cancelled but active until period end
    PastDue,    // Payment failed
}
```

### State Machine

```
                                    ┌───────────────┐
                                    │               │
                  ┌─────────────────▶    Active     │◀──────────────────┐
                  │                 │               │                   │
                  │                 └───────┬───────┘                   │
                  │                         │                           │
                  │            ┌────────────┼────────────┐              │
                  │            │            │            │              │
                  │            ▼            │            ▼              │
             payment      ┌─────────┐       │      ┌───────────┐   payment
             succeeds     │         │       │      │           │   succeeds
                  │       │ PastDue │       │      │ Cancelled │        │
                  │       │         │       │      │           │        │
                  │       └────┬────┘       │      └─────┬─────┘        │
                  │            │            │            │              │
                  │            │    user    │     period │              │
                  └────────────┘  cancels   │       ends │              │
                                            │            │              │
                                            ▼            ▼              │
                                      ┌─────────────────────┐           │
                                      │                     │           │
                                      │   (No Subscription) │           │
                                      │                     │    user   │
                                      └─────────────────────┘ subscribes
                                                                        │
                                                ┌───────────────────────┘
                                                │
                                                │
```

### Transitions

| From       | Event              | To         | Action                           |
|------------|--------------------|------------|----------------------------------|
| (none)     | User subscribes    | Active     | Create subscription, grant credits |
| Active     | User cancels       | Cancelled  | Mark cancelled, keep until period end |
| Active     | Payment fails      | PastDue    | Notify user, retry payment       |
| Cancelled  | Period ends        | (none)     | Remove subscription              |
| Cancelled  | User resubscribes  | Active     | Reset to active                  |
| PastDue    | Payment succeeds   | Active     | Resume service                   |
| PastDue    | Grace period ends  | (none)     | Remove subscription              |

## Plan

Available billing plans with pricing and benefits.

### Definition

```rust
pub enum Plan {
    Free,       // $0/month, pay-as-you-go only
    Standard,   // $20/month, 2500 credits/month
    Pro,        // $50/month, 6000 credits/month
    Enterprise, // Custom pricing
}
```

### Plan Comparison

| Plan       | Monthly Price | Monthly Credits | Purchase Discount |
|------------|---------------|-----------------|-------------------|
| Free       | $0            | 0               | 0%                |
| Standard   | $20           | 2,500           | 10%               |
| Pro        | $50           | 6,000           | 20%               |
| Enterprise | Custom        | Custom          | Custom            |

### Plan Methods

```rust
impl Plan {
    /// Monthly credit allowance
    pub const fn monthly_credits(&self) -> i64 {
        match self {
            Plan::Standard => 2500,
            Plan::Pro => 6000,
            Plan::Free | Plan::Enterprise => 0,
        }
    }
    
    /// Discount percentage for one-time purchases
    pub const fn purchase_discount_percent(&self) -> u8 {
        match self {
            Plan::Standard => 10,
            Plan::Pro => 20,
            Plan::Free | Plan::Enterprise => 0,
        }
    }
    
    /// Monthly price in cents
    pub const fn monthly_price_cents(&self) -> i64 {
        match self {
            Plan::Standard => 2000,  // $20
            Plan::Pro => 5000,       // $50
            Plan::Free | Plan::Enterprise => 0,
        }
    }
}
```

### Subscription Credit Grant Flow

```
┌──────────────────────────────────────────────────────────────────┐
│                    Monthly Billing Cycle                         │
└──────────────────────────────────────────────────────────────────┘

  Period Start                                          Period End
       │                                                     │
       ▼                                                     ▼
       ┌─────────────────────────────────────────────────────┐
       │                                                     │
       │  1. Charge subscription fee (via Stripe)            │
       │  2. Grant monthly credits to balance                │
       │  3. Create SubscriptionGrant transaction            │
       │  4. Update current_period_start/end                 │
       │                                                     │
       └─────────────────────────────────────────────────────┘
```

## AutoRefill

Automatic credit purchases when balance drops below a threshold.

### Structure

```rust
pub struct AutoRefill {
    /// Whether auto-refill is enabled
    pub enabled: bool,
    
    /// Trigger refill when balance drops below this amount (in cents)
    pub trigger_below_cents: i64,
    
    /// Amount to refill (in cents)
    pub refill_amount_cents: i64,
}
```

### Default Values

```rust
impl Default for AutoRefill {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger_below_cents: 500,   // $5
            refill_amount_cents: 2500,  // $25
        }
    }
}
```

### Auto-Refill Flow

```
┌─────────────┐    balance drops     ┌──────────────────┐
│             │    below threshold   │                  │
│  Usage      │─────────────────────▶│  Check AutoRefill│
│  Deduction  │                      │  Configuration   │
│             │                      │                  │
└─────────────┘                      └────────┬─────────┘
                                              │
                                              │ if enabled
                                              ▼
                                     ┌──────────────────┐
                                     │                  │
                                     │  Charge Stripe   │
                                     │  (saved card)    │
                                     │                  │
                                     └────────┬─────────┘
                                              │
                                              │ on success
                                              ▼
                                     ┌──────────────────┐
                                     │                  │
                                     │  Add Credits     │
                                     │  + Transaction   │
                                     │  (AutoRefill)    │
                                     │                  │
                                     └──────────────────┘
```

### Trigger Condition

```rust
fn should_trigger_auto_refill(account: &Account) -> bool {
    account.auto_refill
        .as_ref()
        .is_some_and(|ar| ar.enabled && account.balance_cents < ar.trigger_below_cents)
}
```

## Integration IDs

Accounts store external service identifiers for integration:

| Field                 | Service | Purpose                          |
|-----------------------|---------|----------------------------------|
| `lago_customer_id`    | Lago    | Usage reporting, subscriptions   |
| `stripe_customer_id`  | Stripe  | Payment processing, checkout     |

These IDs are created lazily when the user first interacts with each service.
