//! Live API integration tests.
//!
//! These tests run against a real running z-billing service instance.
//! Set the `Z_BILLING_URL` environment variable to the service URL.
//!
//! Run with: cargo test --test live_api -- --nocapture --ignored

use reqwest::Client;
use serde_json::json;
use uuid::Uuid;

fn get_base_url() -> String {
    std::env::var("Z_BILLING_URL").unwrap_or_else(|_| "http://localhost:8081".to_string())
}

fn generate_test_user_id() -> String {
    Uuid::new_v4().to_string()
}

fn user_auth_header(user_id: &str) -> String {
    format!("Bearer test-token:{user_id}")
}

// ============================================================================
// Health
// ============================================================================

#[tokio::test]
#[ignore] // Run with --ignored flag
async fn live_health_check() {
    let client = Client::new();
    let base_url = get_base_url();
    let url = format!("{base_url}/health");

    println!("Testing: GET {url}");

    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");
    let status = response.status();
    let text = response.text().await.unwrap();

    println!("Status: {status}");
    println!("Response: {text}");

    assert!(
        status.is_success(),
        "Health check failed with status {status}"
    );

    let body: serde_json::Value = serde_json::from_str(&text).expect("Invalid JSON");
    assert_eq!(body["status"], "ok", "Expected status 'ok'");
    assert_eq!(
        body["service"], "z-billing",
        "Expected service 'z-billing' - is the service running the latest code?"
    );
}

// ============================================================================
// Accounts
// ============================================================================

#[tokio::test]
#[ignore]
async fn live_create_and_get_account() {
    let client = Client::new();
    let base_url = get_base_url();
    let user_id = generate_test_user_id();
    let auth = user_auth_header(&user_id);

    println!("Testing with user: {user_id}");
    println!("Auth header: {auth}");

    // Create account
    let response = client
        .post(format!("{base_url}/v1/accounts"))
        .header("Authorization", &auth)
        .json(&json!({}))
        .send()
        .await
        .expect("Failed to create account");

    let status = response.status();
    let text = response.text().await.unwrap();
    println!("Create response status: {status}");
    println!("Create response body: {text}");

    assert!(
        status.is_success(),
        "Create account failed with status {status}: {text}"
    );
    println!("Account created for user: {user_id}");

    // Get account
    let response = client
        .get(format!("{base_url}/v1/accounts/me"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("Failed to get account");

    let status = response.status();
    let text = response.text().await.unwrap();
    println!("Get response status: {status}");
    println!("Get response body: {text}");

    assert!(
        status.is_success(),
        "Get account failed with {status}: {text}"
    );
    let body: serde_json::Value = serde_json::from_str(&text).unwrap();
    println!("Account: {body:#}");
    assert_eq!(body["user_id"], user_id);
}

// ============================================================================
// Credits Flow
// ============================================================================

#[tokio::test]
#[ignore]
async fn live_credits_flow() {
    let client = Client::new();
    let base_url = get_base_url();
    let user_id = generate_test_user_id();
    let auth = user_auth_header(&user_id);

    println!("=== Credits Flow Test ===");
    println!("User ID: {user_id}");
    println!("Base URL: {base_url}");

    // 1. Create account
    let response = client
        .post(format!("{base_url}/v1/accounts"))
        .header("Authorization", &auth)
        .json(&json!({}))
        .send()
        .await
        .expect("Failed to create account");
    let status = response.status();
    let text = response.text().await.unwrap();
    println!("1. Create account: status={status}, body={text}");
    assert!(status.is_success(), "Create account failed: {text}");

    // 2. Check initial balance
    let response = client
        .get(format!("{base_url}/v1/credits/balance"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("Failed to get balance");
    let status = response.status();
    let text = response.text().await.unwrap();
    println!("2. Initial balance: status={status}, body={text}");
    let body: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["balance_cents"], 0);

    // 3. Add credits (admin endpoint)
    let response = client
        .post(format!("{base_url}/v1/credits/add"))
        .json(&json!({
            "user_id": user_id,
            "amount_cents": 10000,
            "reason": "Live test credits"
        }))
        .send()
        .await
        .expect("Failed to add credits");
    let status = response.status();
    let text = response.text().await.unwrap();
    println!("3. Add credits: status={status}, body={text}");
    assert!(status.is_success(), "Add credits failed: {text}");

    // 4. Check updated balance
    let response = client
        .get(format!("{base_url}/v1/credits/balance"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("Failed to get balance");
    let body: serde_json::Value = response.json().await.unwrap();
    println!("4. New balance: {}", body["balance_cents"]);
    assert_eq!(body["balance_cents"], 10000);

    // 5. List transactions
    let response = client
        .get(format!("{base_url}/v1/credits/transactions"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("Failed to list transactions");
    let body: serde_json::Value = response.json().await.unwrap();
    let transactions = body["transactions"].as_array().unwrap();
    println!("5. Transactions count: {}", transactions.len());
    assert!(!transactions.is_empty());

    println!("\n✓ Credits flow test passed!");
}

// ============================================================================
// Usage Reporting Flow
// ============================================================================

#[tokio::test]
#[ignore]
async fn live_usage_flow() {
    let client = Client::new();
    let base_url = get_base_url();
    let user_id = generate_test_user_id();
    let auth = user_auth_header(&user_id);
    let service_api_key =
        std::env::var("Z_BILLING_SERVICE_KEY").unwrap_or_else(|_| "test-service-key".to_string());

    // 1. Create account and fund it
    client
        .post(format!("{base_url}/v1/accounts"))
        .header("Authorization", &auth)
        .json(&json!({}))
        .send()
        .await
        .expect("Failed to create account");
    println!("1. Account created");

    client
        .post(format!("{base_url}/v1/credits/add"))
        .json(&json!({
            "user_id": user_id,
            "amount_cents": 50000,
            "reason": "Live test funding"
        }))
        .send()
        .await
        .expect("Failed to add credits");
    println!("2. Account funded with 50000 credits");

    // 3. Check balance (service auth)
    let response = client
        .post(format!("{base_url}/v1/usage/check"))
        .header("X-API-Key", &service_api_key)
        .header("X-Service-Name", "live-test")
        .json(&json!({
            "user_id": user_id,
            "required_cents": 1000
        }))
        .send()
        .await
        .expect("Failed to check balance");
    let body: serde_json::Value = response.json().await.unwrap();
    println!("3. Balance check: sufficient={}", body["sufficient"]);
    assert_eq!(body["sufficient"], true);

    // 4. Report LLM usage
    let event_id = format!("live-test-{}", Uuid::new_v4());
    let response = client
        .post(format!("{base_url}/v1/usage"))
        .header("X-API-Key", &service_api_key)
        .header("X-Service-Name", "aura-runtime")
        .json(&json!({
            "event_id": event_id,
            "user_id": user_id,
            "metric": {
                "type": "llm_tokens",
                "provider": "anthropic",
                "model": "claude-3-5-sonnet",
                "input_tokens": 5000,
                "output_tokens": 2000
            }
        }))
        .send()
        .await
        .expect("Failed to report usage");

    assert!(
        response.status().is_success(),
        "Report usage failed: {}",
        response.text().await.unwrap()
    );
    let body: serde_json::Value = response.json().await.unwrap();
    println!(
        "4. Usage reported: cost_cents={}, balance_cents={}",
        body["cost_cents"], body["balance_cents"]
    );

    // 5. Report compute usage
    let event_id = format!("live-test-{}", Uuid::new_v4());
    let response = client
        .post(format!("{base_url}/v1/usage"))
        .header("X-API-Key", &service_api_key)
        .header("X-Service-Name", "aura-runtime")
        .json(&json!({
            "event_id": event_id,
            "user_id": user_id,
            "metric": {
                "type": "compute",
                "cpu_hours": 2.5,
                "memory_gb_hours": 5.0
            }
        }))
        .send()
        .await
        .expect("Failed to report compute usage");
    let body: serde_json::Value = response.json().await.unwrap();
    println!(
        "5. Compute usage: cost_cents={}, balance_cents={}",
        body["cost_cents"], body["balance_cents"]
    );

    // 6. Final balance check
    let response = client
        .get(format!("{base_url}/v1/credits/balance"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("Failed to get balance");
    let body: serde_json::Value = response.json().await.unwrap();
    println!("6. Final balance: {}", body["balance_cents"]);
    assert!(body["balance_cents"].as_i64().unwrap() < 50000);

    println!("\n✓ Usage flow test passed!");
}

// ============================================================================
// Batch Usage
// ============================================================================

#[tokio::test]
#[ignore]
async fn live_batch_usage() {
    let client = Client::new();
    let base_url = get_base_url();
    let user_id = generate_test_user_id();
    let auth = user_auth_header(&user_id);
    let service_api_key =
        std::env::var("Z_BILLING_SERVICE_KEY").unwrap_or_else(|_| "test-service-key".to_string());

    // Setup account
    client
        .post(format!("{base_url}/v1/accounts"))
        .header("Authorization", &auth)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{base_url}/v1/credits/add"))
        .json(&json!({
            "user_id": user_id,
            "amount_cents": 100000,
            "reason": "Batch test funding"
        }))
        .send()
        .await
        .unwrap();
    println!("Account created and funded");

    // Batch usage
    let response = client
        .post(format!("{base_url}/v1/usage/batch"))
        .header("X-API-Key", &service_api_key)
        .header("X-Service-Name", "aura-runtime")
        .json(&json!({
            "events": [
                {
                    "event_id": format!("batch-{}", Uuid::new_v4()),
                    "user_id": user_id,
                    "metric": {
                        "type": "compute",
                        "cpu_hours": 1.0,
                        "memory_gb_hours": 2.0
                    }
                },
                {
                    "event_id": format!("batch-{}", Uuid::new_v4()),
                    "user_id": user_id,
                    "metric": {
                        "type": "api_calls",
                        "endpoint": "/v1/sessions",
                        "count": 100
                    }
                },
                {
                    "event_id": format!("batch-{}", Uuid::new_v4()),
                    "user_id": user_id,
                    "metric": {
                        "type": "llm_tokens",
                        "provider": "openai",
                        "model": "gpt-4o",
                        "input_tokens": 1000,
                        "output_tokens": 500
                    }
                }
            ]
        }))
        .send()
        .await
        .expect("Failed to send batch");

    assert!(
        response.status().is_success(),
        "Batch failed: {}",
        response.text().await.unwrap()
    );
    let body: serde_json::Value = response.json().await.unwrap();
    println!(
        "Batch result: processed={}, failed={}",
        body["processed"], body["failed"]
    );
    assert_eq!(body["processed"], 3);
    assert_eq!(body["failed"], 0);

    println!("\n✓ Batch usage test passed!");
}

// ============================================================================
// Full End-to-End Flow
// ============================================================================

#[tokio::test]
#[ignore]
async fn live_full_e2e_flow() {
    let client = Client::new();
    let base_url = get_base_url();
    let user_id = generate_test_user_id();
    let auth = user_auth_header(&user_id);
    let service_api_key =
        std::env::var("Z_BILLING_SERVICE_KEY").unwrap_or_else(|_| "test-service-key".to_string());

    println!("=== Z-Billing E2E Test ===");
    println!("Base URL: {base_url}");
    println!("Test User: {user_id}");
    println!();

    // Health check
    let response = client
        .get(format!("{base_url}/health"))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success(), "Service not healthy!");
    println!("✓ Service is healthy");

    // Create account
    let response = client
        .post(format!("{base_url}/v1/accounts"))
        .header("Authorization", &auth)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    println!("✓ Account created");

    // Fund account
    client
        .post(format!("{base_url}/v1/credits/add"))
        .json(&json!({
            "user_id": user_id,
            "amount_cents": 25000,
            "reason": "E2E test - initial funding"
        }))
        .send()
        .await
        .unwrap();
    println!("✓ Account funded with $250.00");

    // Configure auto-refill
    let response = client
        .post(format!("{base_url}/v1/credits/auto-refill"))
        .header("Authorization", &auth)
        .json(&json!({
            "enabled": true,
            "trigger_below_cents": 1000,
            "refill_amount_cents": 5000
        }))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    println!("✓ Auto-refill configured");

    // Simulate agent session usage
    for i in 1..=3 {
        let event_id = format!("e2e-session-{i}-{}", Uuid::new_v4());
        let response = client
            .post(format!("{base_url}/v1/usage"))
            .header("X-API-Key", &service_api_key)
            .header("X-Service-Name", "aura-runtime")
            .json(&json!({
                "event_id": event_id,
                "user_id": user_id,
                "metric": {
                    "type": "llm_tokens",
                    "provider": "anthropic",
                    "model": "claude-3-5-sonnet",
                    "input_tokens": 2000 * i,
                    "output_tokens": 1000 * i
                }
            }))
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = response.json().await.unwrap();
        println!(
            "  Session {i}: cost={} cents, balance={} cents",
            body["cost_cents"], body["balance_cents"]
        );
    }
    println!("✓ Simulated 3 agent sessions");

    // Get final state
    let response = client
        .get(format!("{base_url}/v1/accounts/me"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let account: serde_json::Value = response.json().await.unwrap();

    let response = client
        .get(format!("{base_url}/v1/credits/transactions?limit=10"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let txns: serde_json::Value = response.json().await.unwrap();
    let txn_count = txns["transactions"].as_array().unwrap().len();

    println!();
    println!("=== Final State ===");
    println!(
        "Balance: {} cents (${:.2})",
        account["balance_cents"],
        account["balance_cents"].as_i64().unwrap_or(0) as f64 / 100.0
    );
    println!("Plan: {}", account["plan"]);
    println!("Transactions: {txn_count}");

    // Cleanup - delete account
    client
        .delete(format!("{base_url}/v1/accounts/me"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    println!();
    println!("✓ Account deleted (cleanup)");
    println!("\n=== E2E Test Complete ===");
}
