# Z-Billing v0.1.0 - System Overview

Z-Billing is a credit-based billing system for the Cypher Ecosystem and Zero Tech, providing usage tracking, subscription management, and payment processing.

## Z Credit Unit

**1 Z Credit = $0.01 (1 cent)**

- User buys $50 → Gets 5,000 Z Credits
- LLM call costs 3 cents → Deducts 3 Z Credits
- Stored as `i64` (integer cents) to avoid floating point precision issues

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              z-billing                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                      z-billing-service                               │   │
│  │                                                                      │   │
│  │   ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐   │   │
│  │   │ handlers │  │   auth   │  │  routes  │  │   integrations   │   │   │
│  │   │          │  │          │  │          │  │                  │   │   │
│  │   │ accounts │  │ ZID JWT  │  │   HTTP   │  │  ┌────────────┐  │   │   │
│  │   │ credits  │  │ API Key  │  │   API    │  │  │   Stripe   │  │   │   │
│  │   │ usage    │  │          │  │          │  │  └────────────┘  │   │   │
│  │   │ webhooks │  │          │  │          │  │  ┌────────────┐  │   │   │
│  │   │          │  │          │  │          │  │  │    Lago    │  │   │   │
│  │   └──────────┘  └──────────┘  └──────────┘  │  └────────────┘  │   │   │
│  │                                             └──────────────────┘   │   │
│  └────────────────────────────────┬────────────────────────────────────┘   │
│                                   │                                         │
│                                   ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                       z-billing-store                                │   │
│  │                                                                      │   │
│  │   Store trait  ──────▶  RocksStore (RocksDB)                        │   │
│  │                                                                      │   │
│  │   Column Families:                                                   │   │
│  │   • accounts            • transactions_by_user                       │   │
│  │   • transactions        • usage_events                               │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                   │                                         │
│                                   ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                        z-billing-core                                │   │
│  │                                                                      │   │
│  │   Domain Types (no I/O):                                            │   │
│  │   • ids      - UserId, TransactionId, AgentId                       │   │
│  │   • account  - Account, Subscription, Plan                          │   │
│  │   • credits  - CreditTransaction, TransactionType                   │   │
│  │   • usage    - UsageEvent, UsageMetric, UsageSource                 │   │
│  │   • pricing  - PricingConfig, LlmPricing                            │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                       z-billing-client                               │   │
│  │                                                                      │   │
│  │   HTTP client library for consuming z-billing API                   │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                        z-billing-lago                                │   │
│  │                                                                      │   │
│  │   Lago deployment management via Docker Compose:                    │   │
│  │   • LagoDeployment  - start, stop, restart, status, logs            │   │
│  │   • LagoConfig      - configuration and environment variables       │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Crate Dependencies

```
z-billing-service ───▶ z-billing-store ───▶ z-billing-core
        │                                          ▲
        └──────────────────────────────────────────┘
   
z-billing-client ───▶ z-billing-core

z-billing-lago (standalone - Docker Compose tooling)
```

| Crate               | Purpose                                         |
|---------------------|-------------------------------------------------|
| `z-billing-core`    | Domain types, no I/O (pure Rust)                |
| `z-billing-store`   | RocksDB persistence layer, `Store` trait        |
| `z-billing-service` | HTTP API server with Axum, integrations         |
| `z-billing-client`  | HTTP client library for consumers               |
| `z-billing-lago`    | Lago deployment management via Docker Compose   |

## Core Concepts

### Accounts

Each user has a single billing account that tracks:
- **Balance**: Current Z Credit balance (in cents)
- **Lifetime stats**: Purchased, granted, and used credits
- **Subscription**: Optional plan with monthly credit grants
- **Auto-refill**: Automatic credit purchases when balance is low

### Credits

Credits flow through the system via transactions:
- **Purchase**: User buys credits via Stripe checkout
- **Usage**: Services deduct credits for LLM calls, compute, etc.
- **Subscription Grant**: Monthly credit allowance from plans
- **Refund/Bonus**: Manual adjustments

### Pricing

Usage costs are calculated based on:
- **LLM tokens**: Per-model pricing (input/output tokens)
- **Compute**: CPU-hours and memory GB-hours
- **API calls**: Per-request pricing
- **Storage**: GB-months

## Integration Points

### ZERO-ID (ZID)

Authentication provider for end users:
- JWT tokens validated via JWKS endpoint
- User ID extracted from `sub` claim
- Expected audience: `z-billing`

### Stripe

Payment processing:
- Customer management
- Checkout sessions for credit purchases
- Webhook handling for payment events

### Lago

Usage analytics and subscription management:
- Usage event forwarding
- Subscription lifecycle events

## Technology Stack

| Component      | Technology                        |
|----------------|-----------------------------------|
| Language       | Rust 2021 Edition                 |
| HTTP Framework | Axum                              |
| Database       | RocksDB                           |
| Serialization  | serde, serde_json, ciborium       |
| Async Runtime  | Tokio                             |
| Authentication | jsonwebtoken (JWT/JWKS)           |
