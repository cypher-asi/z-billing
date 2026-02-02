//! Credit balance and transactions integration tests.

mod common;

use common::TestHarness;
use serde_json::json;

// ============================================================================
// Balance
// ============================================================================

#[tokio::test]
async fn get_balance_success() {
    let harness = TestHarness::new();

    // Create account first
    harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await
        .assert_status_ok();

    // Get balance
    let response = harness
        .server
        .get("/v1/credits/balance")
        .add_header("authorization", harness.user_auth_header())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["balance_cents"], 0);
}

#[tokio::test]
async fn get_balance_without_account_fails() {
    let harness = TestHarness::new();

    let response = harness
        .server
        .get("/v1/credits/balance")
        .add_header("authorization", harness.user_auth_header())
        .await;

    response.assert_status_not_found();
}

#[tokio::test]
async fn get_balance_without_auth_fails() {
    let harness = TestHarness::new();

    let response = harness.server.get("/v1/credits/balance").await;

    response.assert_status_unauthorized();
}

// ============================================================================
// Transactions
// ============================================================================

#[tokio::test]
async fn list_transactions_empty() {
    let harness = TestHarness::new();

    // Create account first
    harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await
        .assert_status_ok();

    // List transactions
    let response = harness
        .server
        .get("/v1/credits/transactions")
        .add_header("authorization", harness.user_auth_header())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["transactions"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn list_transactions_with_pagination() {
    let harness = TestHarness::new();

    // Create account first
    harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await
        .assert_status_ok();

    // List transactions with pagination params
    let response = harness
        .server
        .get("/v1/credits/transactions?limit=10&offset=0")
        .add_header("authorization", harness.user_auth_header())
        .await;

    response.assert_status_ok();
}

// ============================================================================
// Admin Add Credits
// ============================================================================

#[tokio::test]
async fn admin_add_credits_success() {
    let harness = TestHarness::new();

    // Create account first
    harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await
        .assert_status_ok();

    // Admin adds credits
    // Note: This endpoint currently doesn't require auth (should be fixed in production)
    let response = harness
        .server
        .post("/v1/credits/add")
        .json(&json!({
            "user_id": harness.test_user_id.to_string(),
            "amount_cents": 5000,
            "reason": "Test bonus credits"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["balance_cents"], 5000);

    // Verify balance
    let response = harness
        .server
        .get("/v1/credits/balance")
        .add_header("authorization", harness.user_auth_header())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["balance_cents"], 5000);
}

#[tokio::test]
async fn admin_add_credits_invalid_user_fails() {
    let harness = TestHarness::new();

    let response = harness
        .server
        .post("/v1/credits/add")
        .json(&json!({
            "user_id": "invalid-uuid",
            "amount_cents": 5000,
            "reason": "Test"
        }))
        .await;

    response.assert_status_bad_request();
}

#[tokio::test]
async fn admin_add_credits_nonexistent_account_fails() {
    let harness = TestHarness::new();
    // Don't create an account

    let response = harness
        .server
        .post("/v1/credits/add")
        .json(&json!({
            "user_id": harness.test_user_id.to_string(),
            "amount_cents": 5000,
            "reason": "Test"
        }))
        .await;

    response.assert_status_not_found();
}

// ============================================================================
// Purchase (requires Stripe configuration)
// ============================================================================

#[tokio::test]
#[ignore = "requires Stripe API key"]
async fn purchase_credits_returns_checkout_url() {
    let harness = TestHarness::new();

    // Create account first
    harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await
        .assert_status_ok();

    // Request purchase
    let response = harness
        .server
        .post("/v1/credits/purchase")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({
            "amount_usd": 10.0
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["checkout_url"].as_str().is_some());
    assert!(body["session_id"].as_str().is_some());
}

// ============================================================================
// Auto-Refill
// ============================================================================

#[tokio::test]
async fn configure_auto_refill_success() {
    let harness = TestHarness::new();

    // Create account first
    harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await
        .assert_status_ok();

    // Configure auto-refill
    let response = harness
        .server
        .post("/v1/credits/auto-refill")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({
            "enabled": true,
            "trigger_below_cents": 500,
            "refill_amount_cents": 2500
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["auto_refill"]["enabled"], true);
    assert_eq!(body["auto_refill"]["trigger_below_cents"], 500);
}

#[tokio::test]
async fn disable_auto_refill() {
    let harness = TestHarness::new();

    // Create account first
    harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await
        .assert_status_ok();

    // Disable auto-refill
    let response = harness
        .server
        .post("/v1/credits/auto-refill")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({
            "enabled": false
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["auto_refill"]["enabled"], false);
}
