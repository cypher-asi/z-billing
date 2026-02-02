//! Identifier types for z-billing.
//!
//! This module provides strongly-typed identifiers for users, transactions, and agents.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use ulid::Ulid;

/// A user identifier (UUID format from Zero-ID).
///
/// User IDs are provided by Zero-ID and extracted from JWT `sub` claims.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UserId(uuid::Uuid);

impl UserId {
    /// Create a new `UserId` from a UUID.
    #[must_use]
    pub const fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    /// Generate a new random `UserId` (for testing).
    #[must_use]
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Return the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }

    /// Return the bytes of the UUID.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}

impl FromStr for UserId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uuid = uuid::Uuid::parse_str(s).map_err(|_| IdError::InvalidUuid)?;
        Ok(Self(uuid))
    }
}

impl fmt::Debug for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UserId({})", self.0)
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for UserId {
    type Error = IdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<UserId> for String {
    fn from(id: UserId) -> Self {
        id.0.to_string()
    }
}

impl AsRef<[u8]> for UserId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// A transaction identifier using ULID for time-ordering.
///
/// Transaction IDs are time-ordered to allow efficient range queries
/// and natural chronological sorting.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TransactionId(Ulid);

impl TransactionId {
    /// Create a new `TransactionId` from a ULID.
    #[must_use]
    pub const fn from_ulid(ulid: Ulid) -> Self {
        Self(ulid)
    }

    /// Generate a new `TransactionId` with the current timestamp.
    #[must_use]
    pub fn generate() -> Self {
        Self(Ulid::new())
    }

    /// Return the underlying ULID.
    #[must_use]
    pub const fn as_ulid(&self) -> &Ulid {
        &self.0
    }

    /// Return the bytes of the ULID (16 bytes).
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 16] {
        self.0.to_bytes()
    }

    /// Create a `TransactionId` from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes are invalid.
    pub fn from_bytes(bytes: [u8; 16]) -> Result<Self, IdError> {
        Ok(Self(Ulid::from_bytes(bytes)))
    }
}

impl FromStr for TransactionId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let ulid = Ulid::from_string(s).map_err(|_| IdError::InvalidUlid)?;
        Ok(Self(ulid))
    }
}

impl fmt::Debug for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TransactionId({})", self.0)
    }
}

impl fmt::Display for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for TransactionId {
    type Error = IdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<TransactionId> for String {
    fn from(id: TransactionId) -> Self {
        id.0.to_string()
    }
}

/// An agent identifier (UUID format).
///
/// Agent IDs reference agents in aura-swarm or other services.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AgentId(uuid::Uuid);

impl AgentId {
    /// Create a new `AgentId` from a UUID.
    #[must_use]
    pub const fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    /// Generate a new random `AgentId` (for testing).
    #[must_use]
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Return the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }

    /// Return the bytes of the UUID.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}

impl FromStr for AgentId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uuid = uuid::Uuid::parse_str(s).map_err(|_| IdError::InvalidUuid)?;
        Ok(Self(uuid))
    }
}

impl fmt::Debug for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AgentId({})", self.0)
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for AgentId {
    type Error = IdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<AgentId> for String {
    fn from(id: AgentId) -> Self {
        id.0.to_string()
    }
}

impl AsRef<[u8]> for AgentId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Errors that can occur when parsing identifiers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// The input is not a valid UUID.
    #[error("invalid UUID format")]
    InvalidUuid,

    /// The input is not a valid ULID.
    #[error("invalid ULID format")]
    InvalidUlid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_id_roundtrip() {
        let id = UserId::generate();
        let str_repr = id.to_string();
        let parsed = UserId::from_str(&str_repr).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn user_id_serde_json() {
        let id = UserId::generate();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: UserId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn transaction_id_roundtrip() {
        let id = TransactionId::generate();
        let str_repr = id.to_string();
        let parsed = TransactionId::from_str(&str_repr).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn transaction_id_serde_json() {
        let id = TransactionId::generate();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: TransactionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn transaction_id_bytes_roundtrip() {
        let id = TransactionId::generate();
        let bytes = id.to_bytes();
        let parsed = TransactionId::from_bytes(bytes).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn agent_id_roundtrip() {
        let id = AgentId::generate();
        let str_repr = id.to_string();
        let parsed = AgentId::from_str(&str_repr).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn agent_id_serde_json() {
        let id = AgentId::generate();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: AgentId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }
}
