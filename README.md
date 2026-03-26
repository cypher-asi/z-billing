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

## API Reference

See [docs/api.md](docs/api.md) for the full API reference.

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

## Storage

PostgreSQL database with three tables:

- **accounts** — User billing accounts (balance, subscription, Stripe/Lago IDs)
- **credit_transactions** — Immutable ledger of all balance changes (ULID-ordered)
- **usage_events** — Usage events for idempotency checking

Atomic operations: credit deduction uses `SELECT ... FOR UPDATE` row locking within a PostgreSQL transaction to prevent overdraft.

---

## License

MIT
