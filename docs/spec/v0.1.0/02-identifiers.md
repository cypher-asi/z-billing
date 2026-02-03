# Z-Billing v0.1.0 - Identifier Types

This document specifies the identifier types used throughout z-billing.

## Overview

Z-Billing uses strongly-typed identifiers to prevent accidental mixing of ID types:

| Type            | Format | Length   | Source                     |
|-----------------|--------|----------|----------------------------|
| `UserId`        | UUID   | 36 chars | Zero-ID (JWT `sub` claim)  |
| `TransactionId` | ULID   | 26 chars | Generated internally       |
| `AgentId`       | UUID   | 36 chars | External (aura-swarm)      |

## UserId

A user identifier in UUID v4 format, provided by Zero-ID.

### Format

```
xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
```

Where:
- `x` = hexadecimal digit (0-9, a-f)
- `4` = UUID version 4
- `y` = variant (8, 9, a, or b)

### Example

```
550e8400-e29b-41d4-a716-446655440000
```

### Source

User IDs are extracted from the `sub` (subject) claim of ZID JWT tokens:

```json
{
  "sub": "550e8400-e29b-41d4-a716-446655440000",
  "aud": "z-billing",
  "iss": "https://zid.zero.tech",
  "exp": 1735689600,
  "iat": 1735603200
}
```

### Rust Definition

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UserId(uuid::Uuid);
```

### Serialization

| Format | Example                                    |
|--------|--------------------------------------------|
| JSON   | `"550e8400-e29b-41d4-a716-446655440000"`   |
| Binary | 16 bytes (UUID bytes)                      |

### Operations

```rust
// Parse from string
let user_id: UserId = "550e8400-e29b-41d4-a716-446655440000".parse()?;

// Generate (for testing only)
let user_id = UserId::generate();

// Get underlying UUID
let uuid: &uuid::Uuid = user_id.as_uuid();

// Get bytes (for storage keys)
let bytes: &[u8; 16] = user_id.as_bytes();

// Display
println!("{}", user_id); // 550e8400-e29b-41d4-a716-446655440000
```

## TransactionId

A transaction identifier using ULID (Universally Unique Lexicographically Sortable Identifier).

### Why ULID?

- **Time-ordered**: Lexicographic sorting equals chronological sorting
- **Efficient range queries**: Transactions within a time range can be queried efficiently
- **No coordination needed**: Can be generated independently across services

### Format

```
TTTTTTTTTTRRRRRRRRRRRRRRR
```

Where:
- `T` = timestamp (10 chars, 48 bits, milliseconds since Unix epoch)
- `R` = randomness (16 chars, 80 bits)

### Character Set

Crockford's Base32: `0123456789ABCDEFGHJKMNPQRSTVWXYZ`

### Example

```
01ARZ3NDEKTSV4RRFFQ69G5FAV
│         │               │
│         │               └── Randomness (80 bits)
│         └── Timestamp boundary
└── Timestamp (48 bits)
```

### Rust Definition

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TransactionId(Ulid);
```

### Serialization

| Format | Example                        |
|--------|--------------------------------|
| JSON   | `"01ARZ3NDEKTSV4RRFFQ69G5FAV"` |
| Binary | 16 bytes (ULID bytes)          |

### Operations

```rust
// Generate with current timestamp
let tx_id = TransactionId::generate();

// Parse from string
let tx_id: TransactionId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse()?;

// Get underlying ULID
let ulid: &Ulid = tx_id.as_ulid();

// Convert to/from bytes
let bytes: [u8; 16] = tx_id.to_bytes();
let tx_id = TransactionId::from_bytes(bytes)?;

// Display
println!("{}", tx_id); // 01ARZ3NDEKTSV4RRFFQ69G5FAV
```

### Time Ordering

Transactions generated later have lexicographically greater IDs:

```rust
let tx1 = TransactionId::generate();
std::thread::sleep(std::time::Duration::from_millis(1));
let tx2 = TransactionId::generate();

assert!(tx2.to_string() > tx1.to_string()); // Lexicographic comparison
```

## AgentId

An agent identifier in UUID v4 format, referencing agents in aura-swarm or other services.

### Format

Same as `UserId` - standard UUID v4 format.

### Example

```
123e4567-e89b-12d3-a456-426614174000
```

### Rust Definition

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AgentId(uuid::Uuid);
```

### Usage

Agent IDs are used in usage events to track which agent consumed resources:

```json
{
  "event_id": "evt_abc123",
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "agent_id": "123e4567-e89b-12d3-a456-426614174000",
  "metric": { "llm_tokens": { ... } },
  "source": "aura_swarm"
}
```

## Error Types

```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("invalid UUID format")]
    InvalidUuid,
    
    #[error("invalid ULID format")]
    InvalidUlid,
}
```

## Storage Key Encoding

Identifiers are encoded to bytes for use as RocksDB keys:

| Type            | Key Format         | Example (hex)                          |
|-----------------|--------------------|----------------------------------------|
| `UserId`        | 16 bytes (UUID)    | `550e8400e29b41d4a716446655440000`     |
| `TransactionId` | 16 bytes (ULID)    | `018d5e1f6b2c3d4e5f6a7b8c9d0e1f2a`     |

The `AsRef<[u8]>` trait is implemented for use as key bytes:

```rust
fn account_key(user_id: &UserId) -> Vec<u8> {
    let mut key = Vec::with_capacity(17);
    key.push(ACCOUNT_PREFIX);
    key.extend_from_slice(user_id.as_ref());
    key
}
```
