# Z-Billing v0.1.0 - Usage Events

This document specifies the usage event system for tracking resource consumption.

## Overview

Usage events are reported by services (aura-runtime, aura-swarm) to z-billing for:

1. **Credit deduction**: Immediate balance reduction
2. **Analytics forwarding**: Usage data sent to Lago for dashboards
3. **Audit trail**: Record of all resource consumption

## UsageEvent

### Structure

```rust
pub struct UsageEvent {
    /// Unique event ID for idempotency.
    pub event_id: String,

    /// The user being charged.
    pub user_id: UserId,

    /// The agent that generated this usage, if applicable.
    pub agent_id: Option<AgentId>,

    /// Which service reported this usage.
    pub source: UsageSource,

    /// What was used.
    pub metric: UsageMetric,

    /// Quantity used (tokens, hours, etc.).
    pub quantity: f64,

    /// Pre-calculated cost in cents based on current pricing.
    pub cost_cents: i64,

    /// When the usage occurred.
    pub timestamp: DateTime<Utc>,

    /// Additional context (session_id, request_id, etc.).
    pub metadata: serde_json::Value,
}
```

### JSON Example

```json
{
  "event_id": "evt_abc123",
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "agent_id": "123e4567-e89b-12d3-a456-426614174000",
  "source": "aura_runtime",
  "metric": {
    "type": "llm_tokens",
    "provider": "anthropic",
    "model": "claude-3-5-sonnet",
    "direction": "output"
  },
  "quantity": 1500,
  "cost_cents": 15,
  "timestamp": "2025-01-15T10:30:00Z",
  "metadata": {
    "session_id": "sess_xyz",
    "request_id": "req_456"
  }
}
```

## UsageSource

The service that generated the usage event.

### Definition

```rust
pub enum UsageSource {
    /// Aura Swarm agent orchestration.
    AuraSwarm,

    /// Aura Runtime code execution.
    AuraRuntime,

    /// Custom service.
    Custom(String),
}
```

### String Representation

| Variant              | String Value    |
|----------------------|-----------------|
| `AuraSwarm`          | `"aura_swarm"`  |
| `AuraRuntime`        | `"aura_runtime"`|
| `Custom("my-svc")`   | `"my-svc"`      |

## UsageMetric

The type of resource consumed.

### Definition

```rust
pub enum UsageMetric {
    /// LLM token usage.
    LlmTokens {
        provider: LlmProvider,
        model: String,
        direction: TokenDirection,
    },

    /// Compute resources (CPU and memory).
    Compute {
        cpu_hours: f64,
        memory_gb_hours: f64,
    },

    /// API calls.
    ApiCalls {
        endpoint: String,
    },

    /// Storage usage.
    Storage {
        gb_hours: f64,
    },
}
```

### JSON Serialization

```json
// LLM Tokens
{
  "type": "llm_tokens",
  "provider": "anthropic",
  "model": "claude-3-5-sonnet",
  "direction": "output"
}

// Compute
{
  "type": "compute",
  "cpu_hours": 2.5,
  "memory_gb_hours": 4.0
}

// API Calls
{
  "type": "api_calls",
  "endpoint": "/v1/completions"
}

// Storage
{
  "type": "storage",
  "gb_hours": 10.5
}
```

## LlmProvider

Supported LLM providers.

```rust
pub enum LlmProvider {
    Anthropic,        // Claude models
    OpenAi,           // GPT models
    Google,           // Gemini models
    Custom(String),   // Other providers
}
```

| Variant              | String Value   |
|----------------------|----------------|
| `Anthropic`          | `"anthropic"`  |
| `OpenAi`             | `"openai"`     |
| `Google`             | `"google"`     |
| `Custom("mistral")`  | `"mistral"`    |

## TokenDirection

Whether tokens are input (prompt) or output (completion).

```rust
pub enum TokenDirection {
    Input,   // Prompt tokens
    Output,  // Completion tokens
}
```

## Event Factory Methods

### LLM Usage Event

```rust
UsageEvent::llm(
    event_id: "evt_123".to_string(),
    user_id,
    agent_id: Some(agent_id),
    provider: LlmProvider::Anthropic,
    model: "claude-3-5-sonnet".to_string(),
    direction: TokenDirection::Output,
    tokens: 1500,
    cost_cents: 15,
)
```

### Compute Usage Event

```rust
UsageEvent::compute(
    event_id: "evt_456".to_string(),
    user_id,
    agent_id: Some(agent_id),
    cpu_hours: 2.5,
    memory_gb_hours: 4.0,
    cost_cents: 25,
)
```

## Idempotency

The `event_id` field ensures idempotent processing:

- Each event must have a globally unique `event_id`
- If the same `event_id` is submitted twice, the duplicate is rejected
- Services should generate deterministic IDs (e.g., `{request_id}_{metric_type}`)

```
First request:  event_id="evt_123" → Processed, balance deducted
Second request: event_id="evt_123" → Rejected as duplicate, no charge
```

## Usage Processing Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Usage Event Processing                            │
└─────────────────────────────────────────────────────────────────────────────┘

  ┌──────────────────┐
  │  Service         │
  │  (aura-runtime)  │
  └────────┬─────────┘
           │
           │ POST /v1/usage
           │ X-API-Key: <service_key>
           │
           ▼
  ┌──────────────────┐
  │  z-billing       │
  │  Service         │
  └────────┬─────────┘
           │
           ▼
  ┌──────────────────┐     ┌──────────────────┐
  │ Check idempotency│────▶│ Already processed│
  │ (event_id)       │ yes │ → Return 409     │
  └────────┬─────────┘     └──────────────────┘
           │ no
           ▼
  ┌──────────────────┐
  │ Calculate cost   │
  │ if not provided  │
  └────────┬─────────┘
           │
           ▼
  ┌──────────────────┐     ┌──────────────────┐
  │ Check balance    │────▶│ Insufficient     │
  │                  │ low │ → Return 402     │
  └────────┬─────────┘     └──────────────────┘
           │ ok
           ▼
  ┌──────────────────────────────────────────┐
  │            Atomic Transaction            │
  │ ┌──────────────────────────────────────┐ │
  │ │ 1. Deduct from balance               │ │
  │ │ 2. Create credit transaction         │ │
  │ │ 3. Store usage event (idempotency)   │ │
  │ └──────────────────────────────────────┘ │
  └────────┬─────────────────────────────────┘
           │
           ▼
  ┌──────────────────┐
  │ Forward to Lago  │ (async, non-blocking)
  │ for analytics    │
  └────────┬─────────┘
           │
           ▼
  ┌──────────────────┐
  │ Return response  │
  │ {balance, cost,  │
  │  transaction_id} │
  └──────────────────┘
```

## API Request Format

### Single Usage Event

```http
POST /v1/usage
X-API-Key: <service_api_key>
X-Service-Name: aura-runtime
Content-Type: application/json

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

### Batch Usage Events

```http
POST /v1/usage/batch
X-API-Key: <service_api_key>
X-Service-Name: aura-swarm
Content-Type: application/json

{
  "events": [
    {
      "event_id": "evt_001",
      "user_id": "user-uuid-1",
      "metric": { "type": "compute", "cpu_hours": 1.0, "memory_gb_hours": 2.0 }
    },
    {
      "event_id": "evt_002",
      "user_id": "user-uuid-2",
      "metric": { "type": "llm_tokens", "provider": "openai", "model": "gpt-4o", "input_tokens": 1000, "output_tokens": 500 }
    }
  ]
}
```

## Response Format

### Success Response

```json
{
  "success": true,
  "balance_cents": 4700,
  "cost_cents": 15,
  "transaction_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"
}
```

### Batch Response

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

## Lago Integration

Usage events are forwarded to Lago for analytics (non-blocking):

| z-billing Metric | Lago Metric Code     |
|------------------|----------------------|
| LLM Input Tokens | `llm_input_tokens`   |
| LLM Output Tokens| `llm_output_tokens`  |
| CPU Hours        | `cpu_hours`          |
| Memory GB-Hours  | `memory_gb_hours`    |

Lago events include properties for segmentation:
- `provider`, `model`, `agent_id` for LLM usage
- `agent_id` for compute usage

## Balance Check

Services can check if a user has sufficient balance before starting work:

```http
POST /v1/usage/check
X-API-Key: <service_api_key>
Content-Type: application/json

{
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "required_cents": 100
}
```

Response:
```json
{
  "sufficient": true,
  "balance_cents": 4500,
  "required_cents": 100
}
```
