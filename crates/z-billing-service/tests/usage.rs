//! Usage reporting integration tests.

mod common;

use common::TestHarness;
use serde_json::json;

// ============================================================================
// Helper to create a funded account
// ============================================================================

async fn create_funded_account(harness: &TestHarness, balance_cents: i64) {
    // Create account
    harness
        .server
        .post("/v1/accounts")
        .add_header("authorization", harness.user_auth_header())
        .json(&json!({}))
        .await
        .assert_status_ok();

    // Add credits (requires admin auth)
    if balance_cents > 0 {
        harness
            .server
            .post("/v1/credits/add")
            .add_header("x-admin-key", harness.admin_key_header())
            .json(&json!({
                "user_id": harness.test_user_id.to_string(),
                "amount_cents": balance_cents,
                "reason": "Test funding"
            }))
            .await
            .assert_status_ok();
    }
}

// ============================================================================
// Report Usage
// ============================================================================

#[tokio::test]
async fn report_llm_usage_success() {
    let harness = TestHarness::new();
    create_funded_account(&harness, 10000).await;

    let response = harness
        .server
        .post("/v1/usage")
        .add_header("x-api-key", &harness.service_api_key)
        .add_header("x-service-name", "aura-runtime")
        .json(&json!({
            "event_id": "evt_test_001",
            "user_id": harness.test_user_id.to_string(),
            "metric": {
                "type": "llm_tokens",
                "provider": "anthropic",
                "model": "claude-3-5-sonnet",
                "input_tokens": 1000,
                "output_tokens": 500
            }
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert!(body["cost_cents"].as_i64().unwrap() > 0);
    assert!(body["balance_cents"].as_i64().unwrap() < 10000);
}

#[tokio::test]
async fn report_compute_usage_success() {
    let harness = TestHarness::new();
    create_funded_account(&harness, 10000).await;

    let response = harness
        .server
        .post("/v1/usage")
        .add_header("x-api-key", &harness.service_api_key)
        .add_header("x-service-name", "aura-runtime")
        .json(&json!({
            "event_id": "evt_test_002",
            "user_id": harness.test_user_id.to_string(),
            "metric": {
                "type": "compute",
                "cpu_hours": 1.5,
                "memory_gb_hours": 3.0
            }
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn report_api_calls_usage_success() {
    let harness = TestHarness::new();
    create_funded_account(&harness, 10000).await;

    let response = harness
        .server
        .post("/v1/usage")
        .add_header("x-api-key", &harness.service_api_key)
        .add_header("x-service-name", "aura-runtime")
        .json(&json!({
            "event_id": "evt_test_003",
            "user_id": harness.test_user_id.to_string(),
            "metric": {
                "type": "api_calls",
                "endpoint": "/v1/chat",
                "count": 5000
            }
        }))
        .await;

    response.assert_status_ok();
}

#[tokio::test]
async fn report_usage_without_api_key_fails() {
    let harness = TestHarness::new();

    let response = harness
        .server
        .post("/v1/usage")
        .json(&json!({
            "event_id": "evt_test_004",
            "user_id": harness.test_user_id.to_string(),
            "metric": {
                "type": "compute",
                "cpu_hours": 1.0,
                "memory_gb_hours": 2.0
            }
        }))
        .await;

    response.assert_status_unauthorized();
}

#[tokio::test]
async fn report_usage_insufficient_credits_fails() {
    let harness = TestHarness::new();
    // Create account with zero balance
    create_funded_account(&harness, 0).await;

    let response = harness
        .server
        .post("/v1/usage")
        .add_header("x-api-key", &harness.service_api_key)
        .add_header("x-service-name", "aura-runtime")
        .json(&json!({
            "event_id": "evt_test_005",
            "user_id": harness.test_user_id.to_string(),
            "metric": {
                "type": "llm_tokens",
                "provider": "anthropic",
                "model": "claude-3-5-sonnet",
                "input_tokens": 10000,
                "output_tokens": 5000
            }
        }))
        .await;

    // Should fail with payment required or similar
    assert!(response.status_code().is_client_error());
    let body: serde_json::Value = response.json();
    assert!(body["error"]["code"]
        .as_str()
        .unwrap()
        .contains("insufficient"));
}

#[tokio::test]
async fn report_usage_duplicate_event_fails() {
    let harness = TestHarness::new();
    create_funded_account(&harness, 10000).await;

    let event_id = "evt_duplicate_test";

    // First event should succeed
    let response = harness
        .server
        .post("/v1/usage")
        .add_header("x-api-key", &harness.service_api_key)
        .add_header("x-service-name", "aura-runtime")
        .json(&json!({
            "event_id": event_id,
            "user_id": harness.test_user_id.to_string(),
            "metric": {
                "type": "compute",
                "cpu_hours": 1.0,
                "memory_gb_hours": 2.0
            }
        }))
        .await;
    response.assert_status_ok();

    // Duplicate event should fail
    let response = harness
        .server
        .post("/v1/usage")
        .add_header("x-api-key", &harness.service_api_key)
        .add_header("x-service-name", "aura-runtime")
        .json(&json!({
            "event_id": event_id,
            "user_id": harness.test_user_id.to_string(),
            "metric": {
                "type": "compute",
                "cpu_hours": 1.0,
                "memory_gb_hours": 2.0
            }
        }))
        .await;

    assert!(response.status_code().is_client_error());
    let body: serde_json::Value = response.json();
    assert!(body["error"]["code"]
        .as_str()
        .unwrap()
        .contains("duplicate"));
}

#[tokio::test]
async fn report_usage_nonexistent_account_fails() {
    let harness = TestHarness::new();
    // Don't create an account

    let response = harness
        .server
        .post("/v1/usage")
        .add_header("x-api-key", &harness.service_api_key)
        .add_header("x-service-name", "aura-runtime")
        .json(&json!({
            "event_id": "evt_test_nonexistent",
            "user_id": harness.test_user_id.to_string(),
            "metric": {
                "type": "compute",
                "cpu_hours": 1.0,
                "memory_gb_hours": 2.0
            }
        }))
        .await;

    response.assert_status_not_found();
}

// ============================================================================
// Batch Usage
// ============================================================================

#[tokio::test]
async fn report_usage_batch_success() {
    let harness = TestHarness::new();
    create_funded_account(&harness, 50000).await;

    let response = harness
        .server
        .post("/v1/usage/batch")
        .add_header("x-api-key", &harness.service_api_key)
        .add_header("x-service-name", "aura-runtime")
        .json(&json!({
            "events": [
                {
                    "event_id": "evt_batch_001",
                    "user_id": harness.test_user_id.to_string(),
                    "metric": {
                        "type": "compute",
                        "cpu_hours": 1.0,
                        "memory_gb_hours": 2.0
                    }
                },
                {
                    "event_id": "evt_batch_002",
                    "user_id": harness.test_user_id.to_string(),
                    "metric": {
                        "type": "compute",
                        "cpu_hours": 0.5,
                        "memory_gb_hours": 1.0
                    }
                }
            ]
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["processed"], 2);
    assert_eq!(body["failed"], 0);
}

// ============================================================================
// Check Balance
// ============================================================================

#[tokio::test]
async fn check_balance_sufficient() {
    let harness = TestHarness::new();
    create_funded_account(&harness, 10000).await;

    let response = harness
        .server
        .post("/v1/usage/check")
        .add_header("x-api-key", &harness.service_api_key)
        .add_header("x-service-name", "aura-runtime")
        .json(&json!({
            "user_id": harness.test_user_id.to_string(),
            "required_cents": 1000
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["sufficient"], true);
    assert_eq!(body["balance_cents"], 10000);
}

#[tokio::test]
async fn check_balance_insufficient() {
    let harness = TestHarness::new();
    create_funded_account(&harness, 500).await;

    let response = harness
        .server
        .post("/v1/usage/check")
        .add_header("x-api-key", &harness.service_api_key)
        .add_header("x-service-name", "aura-runtime")
        .json(&json!({
            "user_id": harness.test_user_id.to_string(),
            "required_cents": 1000
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["sufficient"], false);
    assert_eq!(body["balance_cents"], 500);
}
