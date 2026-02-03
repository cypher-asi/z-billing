# Z-Billing v0.1.0 - HTTP API

This document specifies the HTTP API routes and request/response schemas.

## Base URL

```
https://billing.zero.tech/
```

Development:
```
http://localhost:8080/
```

## Authentication

See [08-authentication.md](08-authentication.md) for details on:
- ZID JWT tokens for end-user requests
- Service API Keys for service-to-service requests

## Routes Overview

| Method | Path                        | Auth            | Description                |
|--------|-----------------------------|-----------------| ---------------------------|
| GET    | `/health`                   | None            | Health check               |
| POST   | `/v1/accounts`              | ZID JWT         | Create account             |
| GET    | `/v1/accounts/me`           | ZID JWT         | Get current account        |
| DELETE | `/v1/accounts/me`           | ZID JWT         | Delete account             |
| GET    | `/v1/credits/balance`       | ZID JWT         | Get credit balance         |
| GET    | `/v1/credits/transactions`  | ZID JWT         | List transactions          |
| POST   | `/v1/credits/purchase`      | ZID JWT         | Initiate purchase          |
| POST   | `/v1/credits/auto-refill`   | ZID JWT         | Configure auto-refill      |
| POST   | `/v1/credits/add`           | Service API Key | Admin add credits          |
| GET    | `/v1/payments`              | ZID JWT         | List payment history       |
| POST   | `/v1/usage`                 | Service API Key | Report usage event         |
| POST   | `/v1/usage/batch`           | Service API Key | Report multiple events     |
| POST   | `/v1/usage/check`           | Service API Key | Check balance sufficiency  |
| POST   | `/webhooks/stripe`          | Stripe Signature| Stripe webhook             |
| POST   | `/webhooks/lago`            | Lago Signature  | Lago webhook               |

---

## Health

### GET /health

Health check endpoint (no authentication required).

**Response:**
```json
{
  "status": "healthy",
  "timestamp": "2025-01-15T10:30:00Z"
}
```

---

## Accounts

### POST /v1/accounts

Create a new billing account for the authenticated user.

**Request:**
```json
{
  "email": "user@example.com"
}
```

| Field   | Type   | Required | Description                    |
|---------|--------|----------|--------------------------------|
| `email` | string | No       | User email (synced from ZID)   |

**Response (201 Created):**
```json
{
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "balance_cents": 0,
  "balance_formatted": "$0.00",
  "lifetime_purchased_cents": 0,
  "lifetime_granted_cents": 0,
  "lifetime_used_cents": 0,
  "plan": "free",
  "auto_refill_enabled": false,
  "created_at": "2025-01-15T10:30:00Z"
}
```

**Errors:**
- `409 Conflict`: Account already exists

### GET /v1/accounts/me

Get the current user's account.

**Response:**
```json
{
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "balance_cents": 4500,
  "balance_formatted": "$45.00",
  "lifetime_purchased_cents": 10000,
  "lifetime_granted_cents": 5000,
  "lifetime_used_cents": 10500,
  "plan": "standard",
  "auto_refill_enabled": true,
  "created_at": "2024-06-15T10:30:00Z"
}
```

**Errors:**
- `404 Not Found`: Account not found

### DELETE /v1/accounts/me

Delete the current user's account.

**Response:**
```json
{
  "deleted": true
}
```

---

## Credits

### GET /v1/credits/balance

Get current credit balance.

**Response:**
```json
{
  "balance_cents": 4500,
  "balance_formatted": "$45.00",
  "plan": "standard"
}
```

### GET /v1/credits/transactions

List transaction history (newest first).

**Query Parameters:**

| Parameter | Type | Default | Max | Description          |
|-----------|------|---------|-----|----------------------|
| `limit`   | int  | 50      | 100 | Number of results    |
| `offset`  | int  | 0       | -   | Pagination offset    |

**Response:**
```json
{
  "transactions": [
    {
      "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
      "amount_cents": -300,
      "transaction_type": "usage",
      "balance_after_cents": 4700,
      "description": "LLM usage: anthropic claude-3-5-sonnet",
      "created_at": "2025-01-15T10:30:00Z"
    },
    {
      "id": "01ARZ3NDEKTSV4RRFFQ69G5FAU",
      "amount_cents": 5000,
      "transaction_type": "purchase",
      "balance_after_cents": 5000,
      "description": "Purchased $50.00 credits via Stripe",
      "created_at": "2025-01-14T08:00:00Z"
    }
  ],
  "has_more": true
}
```

### POST /v1/credits/purchase

Initiate a credit purchase via Stripe Checkout.

**Request:**
```json
{
  "amount_usd": 50.00
}
```

| Field        | Type  | Required | Constraints        |
|--------------|-------|----------|--------------------|
| `amount_usd` | float | Yes      | Min: 5, Max: 1000  |

**Response:**
```json
{
  "checkout_url": "https://checkout.stripe.com/c/pay/cs_test_...",
  "session_id": "cs_test_abc123"
}
```

**Errors:**
- `400 Bad Request`: Invalid amount

### POST /v1/credits/auto-refill

Configure automatic credit refill.

**Request:**
```json
{
  "enabled": true,
  "trigger_below_cents": 500,
  "refill_amount_cents": 2500
}
```

| Field                 | Type | Required | Default | Constraints      |
|-----------------------|------|----------|---------|------------------|
| `enabled`             | bool | Yes      | -       | -                |
| `trigger_below_cents` | int  | No       | 500     | Min: 100 ($1)    |
| `refill_amount_cents` | int  | No       | 2500    | Min: 500 ($5)    |

**Response:**
```json
{
  "auto_refill": {
    "enabled": true,
    "trigger_below_cents": 500,
    "refill_amount_cents": 2500
  }
}
```

### POST /v1/credits/add (Admin)

Add credits to a user account (bonus/promo). Requires Service API Key.

**Request:**
```json
{
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "amount_cents": 1000,
  "reason": "Welcome bonus"
}
```

**Response:**
```json
{
  "balance_cents": 5500,
  "transaction_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"
}
```

---

## Payments

### GET /v1/payments

List payment history from Stripe.

**Query Parameters:**

| Parameter | Type | Default | Max | Description          |
|-----------|------|---------|-----|----------------------|
| `limit`   | int  | 10      | 100 | Number of results    |

**Response:**
```json
{
  "payments": [
    {
      "id": "pi_abc123",
      "amount_cents": 5000,
      "amount_formatted": "$50.00",
      "currency": "usd",
      "status": "succeeded",
      "description": "Z Credits purchase",
      "created_at": "2025-01-14T08:00:00Z"
    }
  ],
  "has_more": false
}
```

---

## Usage

### POST /v1/usage

Report a single usage event. Requires Service API Key.

**Headers:**
```
X-API-Key: <service_api_key>
X-Service-Name: aura-runtime
```

**Request:**
```json
{
  "event_id": "evt_abc123",
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "agent_id": "123e4567-e89b-12d3-a456-426614174000",
  "metric": {
    "type": "llm_tokens",
    "provider": "anthropic",
    "model": "claude-3-5-sonnet",
    "input_tokens": 500,
    "output_tokens": 1000
  },
  "cost_cents": 15,
  "metadata": {
    "session_id": "sess_xyz"
  }
}
```

**Metric Types:**

LLM Tokens:
```json
{
  "type": "llm_tokens",
  "provider": "anthropic",
  "model": "claude-3-5-sonnet",
  "input_tokens": 500,
  "output_tokens": 1000
}
```

Compute:
```json
{
  "type": "compute",
  "cpu_hours": 2.5,
  "memory_gb_hours": 4.0
}
```

API Calls:
```json
{
  "type": "api_calls",
  "endpoint": "/v1/completions",
  "count": 100
}
```

**Response:**
```json
{
  "success": true,
  "balance_cents": 4485,
  "cost_cents": 15,
  "transaction_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"
}
```

**Errors:**
- `402 Payment Required`: Insufficient credits
- `409 Conflict`: Duplicate event

### POST /v1/usage/batch

Report multiple usage events.

**Request:**
```json
{
  "events": [
    { "event_id": "evt_001", "user_id": "...", "metric": { ... } },
    { "event_id": "evt_002", "user_id": "...", "metric": { ... } }
  ]
}
```

**Response:**
```json
{
  "results": [
    { "event_id": "evt_001", "success": true, "cost_cents": 10 },
    { "event_id": "evt_002", "success": false, "error": "insufficient_credits" }
  ],
  "processed": 1,
  "failed": 1
}
```

### POST /v1/usage/check

Check if a user has sufficient balance.

**Request:**
```json
{
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "required_cents": 100
}
```

**Response:**
```json
{
  "sufficient": true,
  "balance_cents": 4500,
  "required_cents": 100
}
```

---

## Webhooks

### POST /webhooks/stripe

Handle Stripe webhook events. Validates `stripe-signature` header.

**Handled Events:**
- `checkout.session.completed` - Add credits after successful purchase
- `payment_intent.succeeded` - Log successful payment
- `customer.subscription.created` - Handle subscription start
- `customer.subscription.updated` - Handle subscription changes
- `customer.subscription.deleted` - Handle subscription cancellation
- `invoice.payment_failed` - Handle failed payments

**Response:**
```json
{
  "received": true
}
```

### POST /webhooks/lago

Handle Lago webhook events. Validates `x-lago-signature` header.

**Handled Events:**
- `subscription.started` - Grant monthly credits
- `subscription.terminated` - Handle cancellation
- `invoice.created` - Log invoice generation
- `subscription.usage_threshold_reached` - Alert on high usage

**Response:**
```json
{
  "received": true
}
```

---

## Error Response Format

All errors follow a consistent format:

```json
{
  "error": {
    "code": "error_code",
    "message": "Human-readable message",
    "details": { /* optional additional info */ }
  }
}
```

### Error Codes

| Code                  | HTTP Status | Description                        |
|-----------------------|-------------|------------------------------------|
| `unauthorized`        | 401         | Missing or invalid credentials     |
| `forbidden`           | 403         | Insufficient permissions           |
| `not_found`           | 404         | Resource not found                 |
| `bad_request`         | 400         | Invalid request parameters         |
| `conflict`            | 409         | Resource already exists            |
| `insufficient_credits`| 402         | Not enough balance                 |
| `duplicate_event`     | 409         | Event already processed            |
| `internal_error`      | 500         | Server error                       |
| `external_service_error` | 502      | Stripe/Lago unavailable           |

---

## Credit Purchase Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Credit Purchase Flow                                 │
└─────────────────────────────────────────────────────────────────────────────┘

  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
  │  Client  │    │ z-billing│    │  Stripe  │    │ RocksDB  │
  └────┬─────┘    └────┬─────┘    └────┬─────┘    └────┬─────┘
       │               │               │               │
       │ POST /v1/credits/purchase     │               │
       │──────────────▶│               │               │
       │               │               │               │
       │               │ Create Checkout Session       │
       │               │──────────────▶│               │
       │               │               │               │
       │               │◀──────────────│               │
       │               │ session_id, url              │
       │◀──────────────│               │               │
       │ checkout_url  │               │               │
       │               │               │               │
       │ Redirect to Stripe            │               │
       │──────────────────────────────▶│               │
       │               │               │               │
       │               │ Complete payment             │
       │◀──────────────────────────────│               │
       │               │               │               │
       │               │ Webhook: checkout.session.completed
       │               │◀──────────────│               │
       │               │               │               │
       │               │ Add credits   │               │
       │               │──────────────────────────────▶│
       │               │               │               │
       │ Redirect to success page      │               │
       │◀──────────────────────────────│               │
       │               │               │               │
```
