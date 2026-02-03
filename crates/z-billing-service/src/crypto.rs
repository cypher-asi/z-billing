//! Cryptographic utilities for webhook verification.
//!
//! This module provides shared cryptographic functions for verifying webhook
//! signatures from external services like Stripe and Lago.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Compute HMAC-SHA256 and return hex-encoded result.
///
/// # Arguments
///
/// * `secret` - The secret key for HMAC computation
/// * `message` - The message to sign
///
/// # Returns
///
/// A hex-encoded string of the HMAC-SHA256 result (64 characters).
///
/// # Panics
///
/// This function will never panic in practice. The `expect` call is guarded by
/// the invariant that HMAC-SHA256 accepts keys of any size per RFC 2104.
#[must_use]
pub fn hmac_sha256_hex(secret: &str, message: &str) -> String {
    // INVARIANT: HMAC-SHA256 accepts keys of any size per RFC 2104, so
    // `new_from_slice` only fails if the Hmac implementation is broken.
    // This is a library invariant, not a runtime condition.
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC-SHA256 accepts any key size");
    mac.update(message.as_bytes());
    let result = mac.finalize();

    hex::encode(result.into_bytes())
}

/// Constant-time string comparison to prevent timing attacks.
///
/// This function compares two strings in constant time to prevent timing
/// side-channel attacks when verifying cryptographic signatures.
///
/// # Arguments
///
/// * `a` - First string to compare
/// * `b` - Second string to compare
///
/// # Returns
///
/// `true` if the strings are equal, `false` otherwise.
#[must_use]
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_sha256_produces_correct_length() {
        let result = hmac_sha256_hex("key", "The quick brown fox jumps over the lazy dog");
        assert!(!result.is_empty());
        assert_eq!(result.len(), 64); // SHA256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn hmac_sha256_is_deterministic() {
        let result1 = hmac_sha256_hex("secret", "message");
        let result2 = hmac_sha256_hex("secret", "message");
        assert_eq!(result1, result2);
    }

    #[test]
    fn hmac_sha256_different_inputs() {
        let result1 = hmac_sha256_hex("secret", "message1");
        let result2 = hmac_sha256_hex("secret", "message2");
        assert_ne!(result1, result2);
    }

    #[test]
    fn constant_time_eq_equal_strings() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(constant_time_eq("", ""));
        assert!(constant_time_eq("longer string here", "longer string here"));
    }

    #[test]
    fn constant_time_eq_different_strings() {
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(!constant_time_eq("ab", "abc"));
        assert!(!constant_time_eq("abc", "ABC"));
    }
}
