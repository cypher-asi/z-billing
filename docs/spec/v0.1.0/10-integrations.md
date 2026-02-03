# Z-Billing v0.1.0 - External Integrations

This document specifies the Stripe and Lago integrations.

## Overview

| Integration | Purpose                                    |
|-------------|--------------------------------------------|
| **Stripe**  | Payment processing, customer management    |
| **Lago**    | Usage analytics, subscription management   |

## Stripe Integration

### Responsibilities

- Customer registration
- Checkout sessions for credit purchases
- Payment history tracking
- Webhook handling for payment events

### Configuration

| Environment Variable      | Description                          |
|---------------------------|--------------------------------------|
| `STRIPE_API_KEY`          | Stripe secret API key                |
| `STRIPE_WEBHOOK_SECRET`   | Webhook signing secret               |

Or via secrets file (`.secrets/stripe.json`):
```json
{
  "api_key": "sk_test_...",
  "webhook_secret": "whsec_..."
}
```

### StripeClient

```rust
pub struct StripeClient {
    client: Client,
    api_key: String,
    webhook_secret: Option<String>,
}
```

### Customer Management

#### Create Customer

When a new account is created in z-billing:

```rust
stripe.create_customer(
    user_id: "550e8400-e29b-41d4-a716-446655440000",
    email: Some("user@example.com"),
    name: Some("John Doe"),
)
```

Stripe API call:
```http
POST https://api.stripe.com/v1/customers
Content-Type: application/x-www-form-urlencoded

email=user@example.com
name=John+Doe
metadata[user_id]=550e8400-e29b-41d4-a716-446655440000
```

### Checkout Sessions

#### Create Checkout Session

For credit purchases:

```rust
stripe.create_checkout_session(
    customer_id: Some("cus_abc123"),
    user_id: "550e8400-e29b-41d4-a716-446655440000",
    amount_cents: 5000,     // What user pays
    credits_amount: 5000,   // Credits they receive
    success_url: "https://app.zero.tech/billing/success?session_id={CHECKOUT_SESSION_ID}",
    cancel_url: "https://app.zero.tech/billing/cancel",
)
```

Metadata included in session:
- `user_id`: For linking payment to account
- `credits_amount`: Credits to grant on completion

### Webhook Events

#### checkout.session.completed

Triggered when payment completes:

```json
{
  "type": "checkout.session.completed",
  "data": {
    "object": {
      "id": "cs_test_abc123",
      "payment_status": "paid",
      "client_reference_id": "550e8400-e29b-41d4-a716-446655440000",
      "amount_total": 5000,
      "metadata": {
        "credits_amount": "5000"
      }
    }
  }
}
```

Processing:
1. Extract `client_reference_id` (user_id)
2. Extract `credits_amount` from metadata (or fall back to `amount_total`)
3. Create `CreditTransaction::purchase`
4. Call `store.add_credits()`

#### Other Handled Events

| Event                          | Action                           |
|--------------------------------|----------------------------------|
| `payment_intent.succeeded`     | Log successful payment           |
| `customer.subscription.created`| Handle subscription start        |
| `customer.subscription.updated`| Handle subscription changes      |
| `customer.subscription.deleted`| Handle subscription cancellation |
| `invoice.payment_failed`       | Log failed payment, notify user  |

### Stripe Types

```rust
/// Stripe customer
pub struct Customer {
    pub id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub metadata: serde_json::Value,
    pub created: i64,
}

/// Checkout session
pub struct CheckoutSession {
    pub id: String,
    pub url: Option<String>,
    pub payment_status: Option<String>,
    pub customer: Option<String>,
    pub amount_total: Option<i64>,
    pub client_reference_id: Option<String>,
    pub metadata: serde_json::Value,
}

/// Payment intent
pub struct PaymentIntent {
    pub id: String,
    pub amount: i64,
    pub currency: String,
    pub status: String,
    pub customer: Option<String>,
    pub created: i64,
}
```

---

## Lago Integration

### Responsibilities

- Usage event aggregation for analytics dashboards
- Subscription management and billing cycles
- Invoice generation
- Customer organization in Lago's system

**Note:** Z-billing handles actual credit deduction. Lago is used for analytics and reporting.

### Configuration

| Environment Variable      | Description                          |
|---------------------------|--------------------------------------|
| `LAGO_API_URL`            | Lago API endpoint                    |
| `LAGO_API_KEY`            | Lago API authentication key          |
| `LAGO_ORGANIZATION_ID`    | Lago organization identifier         |

Or via secrets file (`.secrets/lago.json`):
```json
{
  "api_url": "http://localhost:3000",
  "api_key": "lago_api_key_...",
  "organization_id": "org_123"
}
```

### LagoClient

```rust
pub struct LagoClient {
    client: Client,
    base_url: String,
    api_key: String,
}
```

### Customer Management

#### Create Customer

When a new account is created:

```rust
lago.create_customer(CustomerInput {
    external_id: user_id.to_string(),
    name: "John Doe".to_string(),
    email: Some("user@example.com".to_string()),
    billing_configuration: Some(BillingConfiguration {
        payment_provider: Some("stripe".to_string()),
        provider_customer_id: Some("cus_stripe_abc".to_string()),
        sync_with_provider: Some(true),
    }),
    metadata: None,
})
```

Lago API call:
```http
POST /api/v1/customers
Authorization: Bearer <api_key>
Content-Type: application/json

{
  "customer": {
    "external_id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "John Doe",
    "email": "user@example.com",
    "billing_configuration": {
      "payment_provider": "stripe",
      "provider_customer_id": "cus_stripe_abc",
      "sync_with_provider": true
    }
  }
}
```

### Subscription Management

#### Create Subscription

```rust
lago.create_subscription(SubscriptionInput {
    external_customer_id: user_id.to_string(),
    plan_code: "standard".to_string(),
    external_id: Some(subscription_id.to_string()),
    name: None,
    billing_time: Some("anniversary".to_string()),
})
```

#### Plan Codes

| Plan Code    | Description                        |
|--------------|------------------------------------|
| `free`       | Free tier, pay-as-you-go           |
| `standard`   | $20/month, 2500 credits            |
| `pro`        | $50/month, 6000 credits            |
| `enterprise` | Custom pricing                     |

### Usage Event Forwarding

Usage events are forwarded to Lago asynchronously for analytics:

```rust
lago.send_event(EventInput {
    transaction_id: event_id.to_string(),
    external_customer_id: user_id.to_string(),
    code: "llm_input_tokens".to_string(),
    timestamp: timestamp.to_string(),
    properties: Some(json!({
        "tokens": 1000,
        "provider": "anthropic",
        "model": "claude-3-5-sonnet",
        "agent_id": "agent-uuid"
    })),
    external_subscription_id: None,
})
```

### Billable Metric Codes

| Metric Code        | Description                    |
|--------------------|--------------------------------|
| `cpu_hours`        | Compute CPU hours              |
| `memory_gb_hours`  | Compute memory GB-hours        |
| `llm_input_tokens` | LLM prompt tokens              |
| `llm_output_tokens`| LLM completion tokens          |

### Convenience Methods

#### send_llm_usage

```rust
lago.send_llm_usage(
    transaction_id: "evt_123",
    customer_id: "user-uuid",
    provider: "anthropic",
    model: "claude-3-5-sonnet",
    agent_id: Some("agent-uuid"),
    input_tokens: 500,
    output_tokens: 1000,
)
```

Creates two events:
- `llm_input_tokens` with input token count
- `llm_output_tokens` with output token count

#### send_compute_usage

```rust
lago.send_compute_usage(
    transaction_id: "evt_456",
    customer_id: "user-uuid",
    agent_id: Some("agent-uuid"),
    cpu_hours: 2.5,
    memory_gb_hours: 4.0,
)
```

Creates two events:
- `cpu_hours` with CPU hours
- `memory_gb_hours` with memory GB-hours

### Webhook Events

#### subscription.started

Grant monthly credits when subscription begins:

```json
{
  "webhook_type": "subscription.started",
  "subscription": {
    "external_customer_id": "550e8400-e29b-41d4-a716-446655440000",
    "plan_code": "standard"
  }
}
```

#### subscription.terminated

Handle subscription cancellation:

```json
{
  "webhook_type": "subscription.terminated",
  "subscription": {
    "external_customer_id": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

#### invoice.created

Log invoice generation:

```json
{
  "webhook_type": "invoice.created",
  "invoice": {
    "lago_id": "inv_abc123",
    "amount_cents": 2000
  }
}
```

---

## Integration Flow Diagrams

### Account Creation Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Account Creation Flow                               │
└─────────────────────────────────────────────────────────────────────────────┘

  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
  │  Client  │    │ z-billing│    │  Stripe  │    │   Lago   │
  └────┬─────┘    └────┬─────┘    └────┬─────┘    └────┬─────┘
       │               │               │               │
       │ POST /v1/accounts            │               │
       │──────────────▶│               │               │
       │               │               │               │
       │               │ Create customer              │
       │               │──────────────▶│               │
       │               │               │               │
       │               │◀──────────────│               │
       │               │ cus_stripe_abc│               │
       │               │               │               │
       │               │ Create customer              │
       │               │──────────────────────────────▶│
       │               │               │               │
       │               │◀──────────────────────────────│
       │               │ lago_cus_xyz │               │
       │               │               │               │
       │               │ Store account (with both IDs)│
       │               │──────────────▶ RocksDB       │
       │               │               │               │
       │◀──────────────│               │               │
       │ Account response             │               │
       │               │               │               │
```

### Usage Processing Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Usage Processing Flow                               │
└─────────────────────────────────────────────────────────────────────────────┘

  ┌──────────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
  │ aura-runtime │    │ z-billing│    │ RocksDB  │    │   Lago   │
  └──────┬───────┘    └────┬─────┘    └────┬─────┘    └────┬─────┘
         │                 │               │               │
         │ POST /v1/usage  │               │               │
         │────────────────▶│               │               │
         │                 │               │               │
         │                 │ Deduct credits│               │
         │                 │ (atomic)      │               │
         │                 │──────────────▶│               │
         │                 │               │               │
         │                 │◀──────────────│               │
         │                 │ new_balance   │               │
         │                 │               │               │
         │◀────────────────│               │               │
         │ Response        │               │               │
         │                 │               │               │
         │                 │ Forward to Lago (async)       │
         │                 │──────────────────────────────▶│
         │                 │               │               │
         │                 │               │ Store for     │
         │                 │               │ analytics     │
         │                 │               │               │
```

### Subscription Credit Grant Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Subscription Credit Grant Flow                           │
└─────────────────────────────────────────────────────────────────────────────┘

  ┌──────────┐    ┌──────────┐    ┌──────────┐
  │   Lago   │    │ z-billing│    │ RocksDB  │
  └────┬─────┘    └────┬─────┘    └────┬─────┘
       │               │               │
       │ Billing cycle │               │
       │ starts        │               │
       │               │               │
       │ Webhook: subscription.started │
       │──────────────▶│               │
       │               │               │
       │               │ Look up plan  │
       │               │ credits       │
       │               │               │
       │               │ Add credits   │
       │               │──────────────▶│
       │               │               │
       │               │ Create        │
       │               │ SubscriptionGrant
       │               │ transaction   │
       │               │               │
       │◀──────────────│               │
       │ acknowledged  │               │
       │               │               │
```

## Error Handling

### Stripe Errors

```rust
pub enum StripeError {
    Http(reqwest::Error),
    Api { status: u16, error: StripeErrorDetail },
    Serialization(serde_json::Error),
    WebhookSignature(String),
    Configuration(String),
}
```

### Lago Errors

```rust
pub enum LagoError {
    Http(reqwest::Error),
    Api { status: u16, error: String, code: Option<String> },
    Serialization(serde_json::Error),
    Configuration(String),
}
```

### Graceful Degradation

Both integrations are optional and fail gracefully:

- Account creation continues even if Stripe/Lago customer creation fails
- Usage processing continues even if Lago forwarding fails (async, non-blocking)
- Warnings are logged for integration failures
