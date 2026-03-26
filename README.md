<h1 align="center">z-billing</h1>

<p align="center">
  <b>Credit-based billing system for the AURA platform.</b>
</p>

## Overview

z-billing is the billing service for AURA. It handles credit accounts, usage-based billing, and payment processing. Users purchase Z Credits which are spent on LLM inference, compute resources, and API calls. aura-router calls z-billing to check balances and debit credits on every LLM request.

---

## Quick Start

### Prerequisites

- Rust 1.75+
- PostgreSQL 15+

### Setup

```
cp .env.example .env
# Edit .env with your database URL and auth config

cargo run --release -p z-billing-service
```

The server starts on `http://0.0.0.0:8080` by default.

### Health Check

```
curl http://localhost:8080/health
```

### Environment Variables

| Variable | Required | Description |
|---|---|---|
| `DATABASE_URL` | Yes | PostgreSQL connection string |
| `LISTEN_ADDR` | No | Bind address (default: `0.0.0.0:8080`, Render uses `0.0.0.0:10000`) |
| `AUTH_BASE_URL` | No | Auth0/ZID domain for JWKS (default: `https://zid.zero.tech`) |
| `AUTH_AUDIENCE` | No | JWT audience (default: `z-billing`) |
| `AUTH_COOKIE_SECRET` | No | Shared secret for HS256 token validation (same as aura-network) |
| `SERVICE_API_KEY` | Yes | Service-to-service auth key (aura-router uses this) |
| `ADMIN_API_KEY` | No | Admin key for manual credit operations |
| `STRIPE_API_KEY` | No | Stripe secret key for payment processing |
| `STRIPE_WEBHOOK_SECRET` | No | Stripe webhook signing secret |
| `LAGO_API_URL` | No | Lago API endpoint for usage reporting |
| `LAGO_API_KEY` | No | Lago API key |
| `FRONTEND_URL` | No | Checkout redirect URL (default: `http://localhost:3000`) |
| `CORS_ORIGINS` | No | Comma-separated allowed origins (default: `*`) |
| `MAX_BODY_BYTES` | No | Max request body size (default: 1MB) |
| `REQUEST_TIMEOUT_SECONDS` | No | Request timeout (default: 30) |

---

## Authentication

**User endpoints** (balance, purchases) accept a JWT in the `Authorization: Bearer <token>` header. Both RS256 (Auth0 JWKS) and HS256 (shared secret) tokens are accepted — same token format as aura-network and aura-storage.

**Service endpoints** (usage reporting) use `X-API-Key` and `X-Service-Name` headers. aura-router uses this to check balances and report usage.

**Admin endpoints** (manual credit operations) use `X-Admin-Key` header.

---

## Core Concepts

### Z Credits

- 1 Z Credit = $0.01 (1 cent)
- Example: $50 purchase = 5,000 credits
- Stored as `i64` integers to avoid floating-point issues

### Accounts

- Auto-created on first usage check or report (zero balance)
- Balance tracking (current + lifetime stats)
- Optional subscription with monthly credit grants
- Auto-refill configuration
- Stripe and Lago customer IDs

### Subscription Plans

| Plan | Monthly Price | Monthly Credits | Purchase Discount |
|------|---------------|-----------------|-------------------|
| Free | $0 | 0 | 0% |
| Standard | $20 | 2,500 | 10% |
| Pro | $50 | 6,000 | 20% |
| Enterprise | Custom | Custom | Custom |

### LLM Pricing (per 1M tokens)

| Model | Input Credits | Output Credits |
|-------|--------------|----------------|
| Claude Sonnet 4.6 | 300 ($3.00) | 1,500 ($15.00) |
| Claude Opus 4.6 | 500 ($5.00) | 2,500 ($25.00) |
| Claude Haiku 4.5 | 100 ($1.00) | 500 ($5.00) |
| GPT-4o | 250 ($2.50) | 1,000 ($10.00) |
| GPT-4o Mini | 15 ($0.15) | 60 ($0.60) |
| Default (unknown) | 100 ($1.00) | 300 ($3.00) |

Minimum charge: 1 credit for any non-zero usage.

---

## API Reference

### Health

| Method | Path | Description | Auth |
|---|---|---|---|
| GET | `/health` | Liveness check | None |

### Accounts

| Method | Path | Description | Auth |
|---|---|---|---|
| POST | `/v1/accounts` | Create account | JWT |
| GET | `/v1/accounts/me` | Get current account | JWT |
| DELETE | `/v1/accounts/me` | Delete account | JWT |

### Credits

| Method | Path | Description | Auth |
|---|---|---|---|
| GET | `/v1/credits/balance` | Get current balance | JWT |
| GET | `/v1/credits/transactions` | Transaction history | JWT |
| POST | `/v1/credits/purchase` | Initiate Stripe checkout. Body: `{"amount_usd": 25.0}` | JWT |
| POST | `/v1/credits/auto-refill` | Configure auto-refill | JWT |
| POST | `/v1/credits/add` | Admin: add credits. Body: `{"user_id": "...", "amount_cents": 5000, "reason": "..."}` | Admin |

### Usage (Service-to-Service)

| Method | Path | Description | Auth |
|---|---|---|---|
| POST | `/v1/usage` | Report usage event and debit credits | Service Key |
| POST | `/v1/usage/batch` | Report multiple usage events | Service Key |
| POST | `/v1/usage/check` | Check if user has sufficient balance | Service Key |

### Payments

| Method | Path | Description | Auth |
|---|---|---|---|
| GET | `/v1/payments` | List Stripe payment history | JWT |

### Webhooks

| Method | Path | Description | Auth |
|---|---|---|---|
| POST | `/webhooks/stripe` | Handle Stripe events | Stripe signature |
| POST | `/webhooks/lago` | Handle Lago events | Lago signature |

### Real-Time

| Protocol | Path | Description | Auth |
|---|---|---|---|
| WebSocket | `/ws/balance` | Real-time balance updates | JWT (query param `?token=`) |

Broadcasts `balance.updated` events when credits are debited (usage), credited (Stripe purchase), or added (admin/subscription).

Event payload:
```json
{
  "type": "balance.updated",
  "userId": "uuid",
  "balanceCents": 5000,
  "balanceFormatted": "$50.00"
}
```

Ping/pong keepalive every 30 seconds.

---

## Request/Response Format

All request and response bodies use JSON.

**Usage check request:**
```json
{
  "user_id": "uuid",
  "required_cents": 100
}
```

**Usage check response:**
```json
{
  "sufficient": true,
  "balance_cents": 5000,
  "required_cents": 100
}
```

**Usage report request:**
```json
{
  "event_id": "unique-id",
  "user_id": "uuid",
  "metric": {
    "type": "llm_tokens",
    "provider": "anthropic",
    "model": "claude-sonnet-4-6",
    "input_tokens": 1000,
    "output_tokens": 500
  }
}
```

**Usage report response:**
```json
{
  "success": true,
  "balance_cents": 4999,
  "cost_cents": 1,
  "transaction_id": "01KM6..."
}
```

---

## Architecture

| Crate | Description |
|---|---|
| **z-billing-core** | Domain types (accounts, credits, pricing, usage events) |
| **z-billing-store** | Storage layer with PostgreSQL backend (`Store` trait) |
| **z-billing-service** | Axum HTTP API server with auth, handlers, Stripe/Lago |
| **z-billing-client** | HTTP client library for service-to-service calls |
| **z-billing-lago** | Lago deployment management via Docker Compose |

---

## Cross-Service Integration

### From aura-router

aura-router calls z-billing on every LLM request:

```
1. Pre-check:    POST /v1/usage/check (verify user has credits)
2. After LLM:    POST /v1/usage (debit credits based on token usage)
```

Uses `X-API-Key` + `X-Service-Name: aura-router` headers.

### Stripe

- Checkout sessions for credit purchases (dynamic pricing, any dollar amount)
- Webhook handling for payment confirmation (`checkout.session.completed`)
- Auto-refill charges

### Lago

- Usage event forwarding for analytics and reporting
- Subscription lifecycle management
- Invoice generation

---

## Storage

PostgreSQL database with three tables:

- **accounts** — User billing accounts (balance, subscription, Stripe/Lago IDs)
- **credit_transactions** — Immutable ledger of all balance changes (ULID-ordered)
- **usage_events** — Usage events for idempotency checking

Atomic operations: credit deduction uses `SELECT ... FOR UPDATE` row locking within a PostgreSQL transaction to prevent overdraft.

---

## License

MIT
