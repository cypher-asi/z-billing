# Z-Billing v0.1.0 - Authentication

This document specifies the authentication mechanisms for z-billing.

## Overview

Z-Billing supports two authentication methods:

| Method            | Use Case                           | Header                  |
|-------------------|------------------------------------|-------------------------|
| **ZID JWT**       | End-user requests (frontend apps)  | `Authorization: Bearer <jwt>` |
| **Service API Key** | Service-to-service (internal)    | `X-API-Key: <key>`      |

## ZERO-ID (ZID) JWT Authentication

### Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      End-User Authentication Flow                           │
└─────────────────────────────────────────────────────────────────────────────┘

  ┌──────────┐       ┌──────────┐       ┌──────────┐       ┌──────────────┐
  │  Client  │       │   ZID    │       │ z-billing│       │   RocksDB    │
  │  (App)   │       │          │       │          │       │              │
  └────┬─────┘       └────┬─────┘       └────┬─────┘       └──────┬───────┘
       │                  │                  │                    │
       │ 1. Login         │                  │                    │
       │─────────────────▶│                  │                    │
       │                  │                  │                    │
       │ 2. JWT Token     │                  │                    │
       │◀─────────────────│                  │                    │
       │                  │                  │                    │
       │ 3. API Request + Bearer Token       │                    │
       │────────────────────────────────────▶│                    │
       │                  │                  │                    │
       │                  │ 4. Validate JWT  │                    │
       │                  │◀─────────────────│                    │
       │                  │   (JWKS fetch)   │                    │
       │                  │─────────────────▶│                    │
       │                  │                  │                    │
       │                  │                  │ 5. Process request │
       │                  │                  │───────────────────▶│
       │                  │                  │                    │
       │ 6. Response      │                  │◀───────────────────│
       │◀────────────────────────────────────│                    │
       │                  │                  │                    │
```

### JWT Structure

```json
{
  "sub": "550e8400-e29b-41d4-a716-446655440000",
  "aud": "z-billing",
  "iss": "https://zid.zero.tech",
  "exp": 1735689600,
  "iat": 1735603200
}
```

| Claim | Description                          | Validation                    |
|-------|--------------------------------------|-------------------------------|
| `sub` | Subject (User ID as UUID)            | Parsed as `UserId`            |
| `aud` | Audience                             | Must be `"z-billing"`         |
| `iss` | Issuer                               | Must be `"https://zid.zero.tech"` |
| `exp` | Expiration time (Unix timestamp)     | Must be in the future         |
| `iat` | Issued at time (Unix timestamp)      | Must be in the past           |

### Configuration

| Environment Variable | Default                    | Description                |
|----------------------|----------------------------|----------------------------|
| `AUTH_BASE_URL`      | `https://zid.zero.tech`    | ZID JWKS endpoint base URL |
| `AUTH_AUDIENCE`      | `z-billing`                | Expected JWT audience      |

### AuthUser Extractor

The `AuthUser` extractor validates JWT tokens and provides user context:

```rust
pub struct AuthUser {
    /// The user ID (from JWT sub claim).
    pub user_id: UserId,
    /// The raw subject claim from the JWT.
    pub subject: String,
}
```

### Usage in Handlers

```rust
pub async fn get_account(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,  // Automatically validated
) -> Result<Json<AccountResponse>, ApiError> {
    let account = state.store
        .get_account(&auth.user_id)?
        .ok_or(ApiError::NotFound)?;
    // ...
}
```

### Test Token Format

For development/testing, a simplified token format is supported:

```
Authorization: Bearer test-token:<user-uuid>
```

Example:
```
Authorization: Bearer test-token:550e8400-e29b-41d4-a716-446655440000
```

## Service API Key Authentication

### Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                   Service-to-Service Authentication Flow                    │
└─────────────────────────────────────────────────────────────────────────────┘

  ┌──────────────┐                      ┌──────────────┐
  │ aura-runtime │                      │  z-billing   │
  │ aura-swarm   │                      │              │
  └──────┬───────┘                      └──────┬───────┘
         │                                     │
         │ POST /v1/usage                      │
         │ X-API-Key: <service_api_key>        │
         │ X-Service-Name: aura-runtime        │
         │─────────────────────────────────────▶│
         │                                     │
         │                  ┌──────────────────┤
         │                  │ Validate API Key │
         │                  │ against config   │
         │                  └──────────────────┤
         │                                     │
         │ Response                            │
         │◀─────────────────────────────────────│
         │                                     │
```

### Headers

| Header           | Required | Description                          |
|------------------|----------|--------------------------------------|
| `X-API-Key`      | Yes      | Service API key for authentication   |
| `X-Service-Name` | No       | Service identifier for logging       |

### Configuration

| Environment Variable | Description                              |
|----------------------|------------------------------------------|
| `SERVICE_API_KEY`    | Shared secret for service authentication |

### ServiceAuth Extractor

```rust
pub struct ServiceAuth {
    /// The service name or identifier.
    pub service_name: String,
}
```

### Usage in Handlers

```rust
pub async fn report_usage(
    State(state): State<Arc<AppState>>,
    auth: ServiceAuth,  // Validates X-API-Key header
    Json(body): Json<UsageRequest>,
) -> Result<Json<UsageResponse>, ApiError> {
    tracing::info!(service = %auth.service_name, "Processing usage");
    // ...
}
```

## Route Protection Matrix

| Route                      | Auth Required      | Method |
|----------------------------|--------------------|--------|
| `GET /health`              | None               | -      |
| `POST /v1/accounts`        | ZID JWT            | User   |
| `GET /v1/accounts/me`      | ZID JWT            | User   |
| `DELETE /v1/accounts/me`   | ZID JWT            | User   |
| `GET /v1/credits/balance`  | ZID JWT            | User   |
| `GET /v1/credits/transactions` | ZID JWT        | User   |
| `POST /v1/credits/purchase` | ZID JWT           | User   |
| `POST /v1/credits/auto-refill` | ZID JWT        | User   |
| `POST /v1/credits/add`     | Service API Key    | Admin  |
| `GET /v1/payments`         | ZID JWT            | User   |
| `POST /v1/usage`           | Service API Key    | Service|
| `POST /v1/usage/batch`     | Service API Key    | Service|
| `POST /v1/usage/check`     | Service API Key    | Service|
| `POST /webhooks/stripe`    | Stripe Signature   | Webhook|
| `POST /webhooks/lago`      | Lago Signature     | Webhook|

## Error Responses

### Unauthorized (401)

Missing or invalid credentials:

```json
{
  "error": {
    "code": "unauthorized",
    "message": "unauthorized"
  }
}
```

### Forbidden (403)

Valid credentials but insufficient permissions:

```json
{
  "error": {
    "code": "forbidden",
    "message": "forbidden"
  }
}
```

## Security Considerations

### JWT Validation

1. **Signature verification**: JWT signed by ZID using RS256
2. **Expiration check**: Token must not be expired
3. **Audience validation**: Must match expected audience
4. **Issuer validation**: Must match expected issuer

### API Key Security

1. **Secure storage**: Key stored as environment variable
2. **Transport security**: HTTPS required in production
3. **Key rotation**: Support for key rotation without downtime
4. **Logging**: Key never logged, only service name

### Webhook Signature Verification

1. **Stripe**: `stripe-signature` header validated against webhook secret
2. **Lago**: `x-lago-signature` header validated (implementation pending)

## JWT Claims Structure

```rust
pub struct JwtClaims {
    /// Subject (user ID).
    pub sub: String,
    /// Audience.
    pub aud: String,
    /// Issuer.
    pub iss: String,
    /// Expiration time (Unix timestamp).
    pub exp: i64,
    /// Issued at (Unix timestamp).
    pub iat: i64,
}
```

## Future Enhancements

- **JWKS caching**: Cache ZID public keys with TTL
- **Scope-based authorization**: Fine-grained permissions in JWT
- **Service-specific keys**: Different API keys per service
- **Key management**: Integration with secrets manager
