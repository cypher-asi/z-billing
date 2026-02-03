//! Account management integration tests.

mod common;

use common::TestHarness;
use serde_json::json;

// ============================================================================
// Account Creation
// ============================================================================

#[tokio::test]
async fn create_account_success() {
    let harness = TestHarness::new();

    let response = harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["user_id"], harness.test_user_id.to_string());
    assert_eq!(body["balance_cents"], 0);
}

#[tokio::test]
async fn create_account_without_auth_fails() {
    let harness = TestHarness::new();

    let response = harness.server.post("/v1/accounts").json(&json!({})).await;

    response.assert_status_unauthorized();
}

#[tokio::test]
async fn create_account_duplicate_fails() {
    let harness = TestHarness::new();

    // Create first account
    let response = harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await;
    response.assert_status_ok();

    // Try to create duplicate
    let response = harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await;

    // Should fail with conflict or bad request
    assert!(response.status_code().is_client_error());
}

// ============================================================================
// Get Account
// ============================================================================

#[tokio::test]
async fn get_account_success() {
    let harness = TestHarness::new();

    // Create account first
    harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await
        .assert_status_ok();

    // Get account
    let response = harness
        .server
        .get("/v1/accounts/me")
        .add_header("authorization", harness.user_auth_header())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["user_id"], harness.test_user_id.to_string());
}

#[tokio::test]
async fn get_nonexistent_account_fails() {
    let harness = TestHarness::new();

    let response = harness
        .server
        .get("/v1/accounts/me")
        .add_header("authorization", harness.user_auth_header())
        .await;

    response.assert_status_not_found();
}

#[tokio::test]
async fn get_account_without_auth_fails() {
    let harness = TestHarness::new();

    let response = harness.server.get("/v1/accounts/me").await;

    response.assert_status_unauthorized();
}

// ============================================================================
// Delete Account
// ============================================================================

#[tokio::test]
async fn delete_account_success() {
    let harness = TestHarness::new();

    // Create account first
    harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await
        .assert_status_ok();

    // Delete account
    let response = harness
        .server
        .delete("/v1/accounts/me")
        .add_header("authorization", harness.user_auth_header())
        .await;

    response.assert_status_ok();

    // Verify account is gone
    let response = harness
        .server
        .get("/v1/accounts/me")
        .add_header("authorization", harness.user_auth_header())
        .await;

    response.assert_status_not_found();
}

#[tokio::test]
async fn delete_nonexistent_account_fails() {
    let harness = TestHarness::new();

    let response = harness
        .server
        .delete("/v1/accounts/me")
        .add_header("authorization", harness.user_auth_header())
        .await;

    response.assert_status_not_found();
}
