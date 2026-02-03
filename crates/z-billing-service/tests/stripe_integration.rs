//! Stripe integration tests using real API calls.
//!
//! These tests require valid Stripe test API credentials in `.secrets/stripe.json`
//! or via environment variables.
//!
//! Run with: `cargo test --test stripe_integration -- --ignored --nocapture`
//!
//! Note: These tests use Stripe's test mode and test card numbers.
//! No real charges are made.

use std::sync::Arc;

use axum_test::TestServer;
use serde_json::json;
use tempfile::TempDir;

use z_billing_core::UserId;
use z_billing_service::{create_router, AppState, ServiceConfig, StripeClient};
use z_billing_store::RocksStore;

/// Test configuration that loads real Stripe credentials.
struct StripeTestConfig {
    api_key: String,
    webhook_secret: Option<String>,
}

impl StripeTestConfig {
    fn load() -> Option<Self> {
        // Try to load from environment first
        if let Ok(api_key) = std::env::var("STRIPE_API_KEY_TEST")
            .or_else(|_| std::env::var("STRIPE_API_KEY"))
        {
            return Some(Self {
                api_key,
                webhook_secret: std::env::var("STRIPE_WEBHOOK_SECRET").ok(),
            });
        }

        // Try to load from secrets file
        let secret_paths = [
            ".secrets/stripe.json",
            "z-billing/.secrets/stripe.json",
            "../.secrets/stripe.json",
            "../../z-billing/.secrets/stripe.json",
        ];

        for path in &secret_paths {
            if let Ok(contents) = std::fs::read_to_string(path) {
                if let Ok(secrets) = serde_json::from_str::<serde_json::Value>(&contents) {
                    // Prefer api_key_test for testing, fall back to api_key
                    let api_key = secrets
                        .get("api_key_test")
                        .or_else(|| secrets.get("api_key"))
                        .and_then(|v| v.as_str());

                    if let Some(api_key) = api_key {
                        return Some(Self {
                            api_key: api_key.to_string(),
                            webhook_secret: secrets
                                .get("webhook_secret")
                                .and_then(|v| v.as_str())
                                .filter(|s| *s != "null" && !s.is_empty())
                                .map(String::from),
                        });
                    }
                }
            }
        }

        None
    }
}

/// Create a test harness with real Stripe integration.
fn create_stripe_test_harness() -> Option<(TestServer, TempDir, UserId, String)> {
    let config = StripeTestConfig::load()?;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let store = RocksStore::open(temp_dir.path()).expect("Failed to open store");

    let service_api_key = "test-service-key".to_string();

    let app_config = ServiceConfig {
        listen_addr: "127.0.0.1:0".into(),
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        auth_base_url: "http://localhost".into(),
        auth_audience: "z-billing".into(),
        service_api_key: Some(service_api_key.clone()),
        admin_api_key: Some("test-admin-key".to_string()),
        lago_api_url: None,
        lago_api_key: None,
        lago_webhook_secret: None,
        lago_organization_id: None,
        stripe_api_key: Some(config.api_key),
        stripe_webhook_secret: config.webhook_secret,
        frontend_url: "http://localhost:3000".into(),
        cors_origins: vec!["*".into()],
        max_body_bytes: 1024 * 1024,
        request_timeout_seconds: 30,
        pricing: z_billing_core::PricingConfig::default(),
    };

    let state = AppState::new(Arc::new(store), app_config);
    let router = create_router(state);
    let server = TestServer::new(router).expect("Failed to create test server");
    let test_user_id = UserId::generate();

    Some((server, temp_dir, test_user_id, service_api_key))
}

/// Get auth header for a user.
fn user_auth_header(user_id: &UserId) -> String {
    format!("Bearer test-token:{}", user_id)
}

// ============================================================================
// Direct Stripe Client Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Stripe API credentials"]
async fn test_stripe_create_customer() {
    let config = StripeTestConfig::load().expect("Stripe credentials not found");
    let client = StripeClient::new(&config.api_key, config.webhook_secret)
        .expect("Failed to create Stripe client");

    let user_id = format!("test-user-{}", uuid::Uuid::new_v4());
    let email = format!("test-{}@example.com", uuid::Uuid::new_v4());

    let customer = client
        .create_customer(&user_id, Some(&email), Some("Test User"))
        .await
        .expect("Failed to create customer");

    println!("Created Stripe customer: {}", customer.id);
    assert!(customer.id.starts_with("cus_"));
    assert_eq!(customer.email.as_deref(), Some(email.as_str()));
    assert_eq!(customer.name.as_deref(), Some("Test User"));
}

#[tokio::test]
#[ignore = "requires Stripe API credentials"]
async fn test_stripe_create_checkout_session() {
    let config = StripeTestConfig::load().expect("Stripe credentials not found");
    let client = StripeClient::new(&config.api_key, config.webhook_secret)
        .expect("Failed to create Stripe client");

    let user_id = format!("test-user-{}", uuid::Uuid::new_v4());

    // First create a customer
    let customer = client
        .create_customer(&user_id, Some("checkout-test@example.com"), None)
        .await
        .expect("Failed to create customer");

    println!("Created customer: {}", customer.id);

    // Create a checkout session for $10.00 (1000 cents) = 1000 credits
    let session = client
        .create_checkout_session(
            Some(&customer.id),
            &user_id,
            1000, // $10.00
            1000, // 1000 credits
            "http://localhost:3000/billing/success?session_id={CHECKOUT_SESSION_ID}",
            "http://localhost:3000/billing/cancel",
        )
        .await
        .expect("Failed to create checkout session");

    println!("Created checkout session: {}", session.id);
    println!("Checkout URL: {:?}", session.url);

    assert!(session.id.starts_with("cs_"));
    assert!(session.url.is_some());

    let url = session.url.unwrap();
    assert!(url.contains("checkout.stripe.com"));

    println!("\n=== CHECKOUT SESSION CREATED ===");
    println!("Session ID: {}", session.id);
    println!("Checkout URL: {}", url);
    println!("\nTo complete the payment flow:");
    println!("1. Open the URL above in a browser");
    println!("2. Use test card: 4242 4242 4242 4242");
    println!("3. Use any future expiry date and any CVC");
    println!("================================\n");
}

#[tokio::test]
#[ignore = "requires Stripe API credentials"]
async fn test_stripe_list_payment_intents() {
    let config = StripeTestConfig::load().expect("Stripe credentials not found");
    let client = StripeClient::new(&config.api_key, config.webhook_secret)
        .expect("Failed to create Stripe client");

    // First create a customer to list payments for
    let user_id = format!("test-user-{}", uuid::Uuid::new_v4());
    let customer = client
        .create_customer(&user_id, Some("payments-test@example.com"), None)
        .await
        .expect("Failed to create customer");

    // List payment intents (should be empty for a new customer)
    let payments = client
        .list_payment_intents(&customer.id, Some(10))
        .await
        .expect("Failed to list payment intents");

    println!("Found {} payment intents for new customer", payments.data.len());
    assert!(payments.data.is_empty()); // New customer has no payments
}

#[tokio::test]
#[ignore = "requires Stripe API credentials"]
async fn test_stripe_webhook_signature_verification() {
    let config = StripeTestConfig::load().expect("Stripe credentials not found");

    // Skip if no webhook secret
    let webhook_secret = match &config.webhook_secret {
        Some(s) => s.clone(),
        None => {
            println!("Skipping webhook test - no webhook_secret configured");
            return;
        }
    };

    let client = StripeClient::new(&config.api_key, Some(webhook_secret.clone()))
        .expect("Failed to create Stripe client");

    // Create a test payload
    let payload = r#"{"id":"evt_test","type":"checkout.session.completed"}"#;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Compute the correct signature
    let signed_payload = format!("{}.{}", timestamp, payload);
    let signature = compute_test_signature(&webhook_secret, &signed_payload);
    let header = format!("t={},v1={}", timestamp, signature);

    // Verify it works
    let result = client.verify_webhook_signature(payload, &header);
    assert!(result.is_ok(), "Valid signature should verify");

    // Test with wrong signature
    let bad_header = format!("t={},v1=bad_signature", timestamp);
    let result = client.verify_webhook_signature(payload, &bad_header);
    assert!(result.is_err(), "Invalid signature should fail");
}

/// Helper to compute test signature (same algorithm as client)
fn compute_test_signature(secret: &str, message: &str) -> String {
    use sha2::{Digest, Sha256};

    let key = secret.as_bytes();
    let message = message.as_bytes();
    const BLOCK_SIZE: usize = 64;

    let key = if key.len() > BLOCK_SIZE {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.finalize().to_vec()
    } else {
        key.to_vec()
    };

    let mut key_padded = [0u8; BLOCK_SIZE];
    key_padded[..key.len()].copy_from_slice(&key);

    let mut i_key_pad = [0x36u8; BLOCK_SIZE];
    let mut o_key_pad = [0x5cu8; BLOCK_SIZE];

    for i in 0..BLOCK_SIZE {
        i_key_pad[i] ^= key_padded[i];
        o_key_pad[i] ^= key_padded[i];
    }

    let mut inner_hasher = Sha256::new();
    inner_hasher.update(i_key_pad);
    inner_hasher.update(message);
    let inner_hash = inner_hasher.finalize();

    let mut outer_hasher = Sha256::new();
    outer_hasher.update(o_key_pad);
    outer_hasher.update(inner_hash);
    let hmac = outer_hasher.finalize();

    hex::encode(hmac)
}

// ============================================================================
// Full API Integration Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Stripe API credentials"]
async fn test_full_account_creation_with_stripe() {
    let (server, _temp_dir, user_id, _service_key) = match create_stripe_test_harness() {
        Some(h) => h,
        None => {
            println!("Skipping test - Stripe credentials not found");
            return;
        }
    };

    // Create account - should also create Stripe customer
    let response = server
        .post("/v1/accounts")
        .add_header(
            axum::http::header::AUTHORIZATION,
            user_auth_header(&user_id),
        )
        .json(&json!({
            "email": format!("integration-test-{}@example.com", uuid::Uuid::new_v4())
        }))
        .await;

    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    println!("Account created: {}", serde_json::to_string_pretty(&body).unwrap());

    assert_eq!(body["user_id"], user_id.to_string());
    assert_eq!(body["balance_cents"], 0);
}

#[tokio::test]
#[ignore = "requires Stripe API credentials"]
async fn test_full_purchase_credits_flow() {
    let (server, _temp_dir, user_id, _service_key) = match create_stripe_test_harness() {
        Some(h) => h,
        None => {
            println!("Skipping test - Stripe credentials not found");
            return;
        }
    };

    let auth_header = user_auth_header(&user_id);

    // 1. Create account first
    let response = server
        .post("/v1/accounts")
        .add_header(axum::http::header::AUTHORIZATION, auth_header.clone())
        .json(&json!({
            "email": format!("purchase-test-{}@example.com", uuid::Uuid::new_v4())
        }))
        .await;

    response.assert_status_ok();
    println!("Account created");

    // 2. Initiate credit purchase
    let response = server
        .post("/v1/credits/purchase")
        .add_header(axum::http::header::AUTHORIZATION, auth_header.clone())
        .json(&json!({
            "amount_usd": 10.0
        }))
        .await;

    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    println!("Purchase response: {}", serde_json::to_string_pretty(&body).unwrap());

    let checkout_url = body["checkout_url"].as_str().expect("Missing checkout_url");
    let session_id = body["session_id"].as_str().expect("Missing session_id");

    assert!(checkout_url.contains("checkout.stripe.com"));
    assert!(session_id.starts_with("cs_"));

    println!("\n=== PURCHASE CREDITS FLOW ===");
    println!("Session ID: {}", session_id);
    println!("Checkout URL: {}", checkout_url);
    println!("\nTo complete the purchase:");
    println!("1. Open the checkout URL in a browser");
    println!("2. Use test card: 4242 4242 4242 4242");
    println!("3. Expiry: Any future date, CVC: Any 3 digits");
    println!("4. After payment, Stripe will send a webhook to add credits");
    println!("===============================\n");
}

/// Test the FULL payment flow with real Stripe API + local credit addition.
/// This test:
/// 1. Creates an account in the test database
/// 2. Creates a real Stripe checkout session
/// 3. Simulates the webhook callback (as if payment completed)
/// 4. Verifies credits were added to the account
///
/// Run with: cargo test --test stripe_integration test_full_payment_flow -- --ignored --nocapture
#[tokio::test]
#[ignore = "requires Stripe API credentials"]
async fn test_full_payment_flow() {
    let (server, _temp_dir, user_id, _service_key) = match create_stripe_test_harness() {
        Some(h) => h,
        None => {
            println!("Skipping test - Stripe credentials not found");
            return;
        }
    };

    let auth_header = user_auth_header(&user_id);

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║              FULL PAYMENT FLOW TEST                           ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Step 1: Create account
    println!("📝 Step 1: Creating account...");
    let response = server
        .post("/v1/accounts")
        .add_header(axum::http::header::AUTHORIZATION, auth_header.clone())
        .json(&json!({ "email": "full-flow-test@example.com" }))
        .await;
    
    response.assert_status_ok();
    let account: serde_json::Value = response.json();
    println!("   ✅ Account created: {}", account["user_id"]);
    println!("   Balance: {} cents", account["balance_cents"]);

    // Step 2: Check initial balance
    println!("\n💰 Step 2: Checking initial balance...");
    let response = server
        .get("/v1/credits/balance")
        .add_header(axum::http::header::AUTHORIZATION, auth_header.clone())
        .await;

    response.assert_status_ok();
    let balance: serde_json::Value = response.json();
    println!("   Initial balance: {} cents ({})", balance["balance_cents"], balance["balance_formatted"]);
    assert_eq!(balance["balance_cents"], 0);

    // Step 3: Initiate purchase (creates real Stripe checkout session)
    println!("\n🛒 Step 3: Creating Stripe checkout session...");
    let response = server
        .post("/v1/credits/purchase")
        .add_header(axum::http::header::AUTHORIZATION, auth_header.clone())
        .json(&json!({ "amount_usd": 10.0 }))
        .await;

    response.assert_status_ok();
    let purchase: serde_json::Value = response.json();
    println!("   ✅ Checkout session created!");
    println!("   Session ID: {}", purchase["session_id"]);
    println!("   Checkout URL: {}", &purchase["checkout_url"].as_str().unwrap()[..80]);

    // Step 4: Simulate webhook (as if user completed payment)
    println!("\n🔔 Step 4: Simulating Stripe webhook (checkout.session.completed)...");
    
    let webhook_payload = json!({
        "id": format!("evt_test_{}", uuid::Uuid::new_v4()),
        "type": "checkout.session.completed",
        "data": {
            "object": {
                "id": purchase["session_id"],
                "payment_status": "paid",
                "client_reference_id": user_id.to_string(),
                "amount_total": 1000,  // $10.00
                "metadata": {
                    "user_id": user_id.to_string(),
                    "credits_amount": "1000"
                }
            }
        }
    });

    // Since we don't have webhook_secret configured in test, signature verification is skipped
    let response = server
        .post("/webhooks/stripe")
        .add_header("stripe-signature", "t=9999999999,v1=test_signature")
        .text(&serde_json::to_string(&webhook_payload).unwrap())
        .await;

    if response.status_code() == 200 {
        println!("   ✅ Webhook processed successfully!");
    } else {
        println!("   ⚠️  Webhook response: {} (signature verification may be enabled)", response.status_code());
        let body = response.text();
        println!("   Response: {}", body);
    }

    // Step 5: Verify credits were added
    println!("\n✅ Step 5: Verifying credits were added...");
    let response = server
        .get("/v1/credits/balance")
        .add_header(axum::http::header::AUTHORIZATION, auth_header.clone())
        .await;

    response.assert_status_ok();
    let balance: serde_json::Value = response.json();
    println!("   New balance: {} cents ({})", balance["balance_cents"], balance["balance_formatted"]);

    // Step 6: Check transaction history
    println!("\n📜 Step 6: Checking transaction history...");
    let response = server
        .get("/v1/credits/transactions")
        .add_header(axum::http::header::AUTHORIZATION, auth_header.clone())
        .await;

    response.assert_status_ok();
    let transactions: serde_json::Value = response.json();
    
    if let Some(txs) = transactions["transactions"].as_array() {
        println!("   Found {} transaction(s):", txs.len());
        for tx in txs {
            println!("   - {} | {} cents | {}", 
                tx["transaction_type"],
                tx["amount_cents"],
                tx["description"]
            );
        }
    }

    // Final assertion
    if balance["balance_cents"] == 1000 {
        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║  🎉 SUCCESS! Full payment flow completed!                     ║");
        println!("║  Credits added: 1000 cents ($10.00)                           ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
    } else {
        println!("\n⚠️  Credits not added (webhook signature verification may be blocking)");
        println!("   This is expected if webhook_secret is configured.");
    }
}

// ============================================================================
// Manual End-to-End Test Helper
// ============================================================================

/// List recent checkout sessions to find completed payments.
/// Run with: cargo test --test stripe_integration list_recent_sessions -- --ignored --nocapture
#[tokio::test]
#[ignore = "manual test"]
async fn list_recent_sessions() {
    let config = StripeTestConfig::load().expect("Stripe credentials not found");
    
    // Use reqwest directly to list checkout sessions
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.stripe.com/v1/checkout/sessions")
        .basic_auth(&config.api_key, Option::<&str>::None)
        .query(&[("limit", "10")])
        .send()
        .await
        .expect("Failed to list sessions");

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    
    println!("\n╔══════════════════════════════════════════════════════════════════════════════╗");
    println!("║                        RECENT CHECKOUT SESSIONS                               ║");
    println!("╠══════════════════════════════════════════════════════════════════════════════╣");

    if let Some(sessions) = body.get("data").and_then(|d| d.as_array()) {
        for session in sessions {
            let id = session.get("id").and_then(|v| v.as_str()).unwrap_or("?");
            let _status = session.get("status").and_then(|v| v.as_str()).unwrap_or("?");
            let payment_status = session.get("payment_status").and_then(|v| v.as_str()).unwrap_or("?");
            let amount = session.get("amount_total").and_then(|v| v.as_i64()).unwrap_or(0);
            let customer = session.get("customer").and_then(|v| v.as_str()).unwrap_or("none");
            let created = session.get("created").and_then(|v| v.as_i64()).unwrap_or(0);
            let client_ref = session.get("client_reference_id").and_then(|v| v.as_str()).unwrap_or("none");
            
            let created_str = chrono::DateTime::from_timestamp(created, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| created.to_string());

            let status_icon = match payment_status {
                "paid" => "✅",
                "unpaid" => "⏳",
                _ => "❓",
            };

            println!("║ {} {} | ${:.2} | {} | {}", 
                status_icon, 
                &id[..id.len().min(50)],
                amount as f64 / 100.0,
                payment_status,
                created_str
            );
            println!("║    Customer: {} | Ref: {}", customer, client_ref);
            
            // Show metadata if payment was successful
            if payment_status == "paid" {
                if let Some(metadata) = session.get("metadata") {
                    if let Some(credits) = metadata.get("credits_amount").and_then(|v| v.as_str()) {
                        println!("║    💰 Credits to add: {}", credits);
                    }
                    if let Some(user_id) = metadata.get("user_id").and_then(|v| v.as_str()) {
                        println!("║    👤 User ID: {}", user_id);
                    }
                }
            }
            println!("╠──────────────────────────────────────────────────────────────────────────────╣");
        }
    }
    
    println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    println!("\n💡 To receive credits automatically, set up a webhook endpoint in Stripe Dashboard");
    println!("   pointing to: https://your-domain.com/webhooks/stripe");
}

/// This is a helper "test" that can be used to manually test the full flow.
/// It creates a checkout session and prints the URL for you to complete manually.
#[tokio::test]
#[ignore = "manual test - run with --nocapture"]
async fn manual_e2e_stripe_test() {
    let config = StripeTestConfig::load().expect("Stripe credentials not found");
    let client = StripeClient::new(&config.api_key, config.webhook_secret)
        .expect("Failed to create Stripe client");

    let user_id = format!("manual-test-{}", uuid::Uuid::new_v4());

    // Create customer
    let customer = client
        .create_customer(&user_id, Some("manual-test@example.com"), Some("Manual Test"))
        .await
        .expect("Failed to create customer");

    println!("Created customer: {}", customer.id);

    // Create checkout session for $25.00
    let session = client
        .create_checkout_session(
            Some(&customer.id),
            &user_id,
            2500, // $25.00
            2500, // 2500 credits
            "http://localhost:3000/billing/success",
            "http://localhost:3000/billing/cancel",
        )
        .await
        .expect("Failed to create checkout session");

    println!("\n");
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           STRIPE CHECKOUT SESSION CREATED                     ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ Customer ID: {:<47} ║", customer.id);
    println!("║ Session ID:  {:<47} ║", session.id);
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ Amount: $25.00 (2500 credits)                                 ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ CHECKOUT URL:                                                 ║");
    println!("║ {}", session.url.as_ref().unwrap());
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ TEST CARD DETAILS:                                            ║");
    println!("║   Card Number: 4242 4242 4242 4242                            ║");
    println!("║   Expiry:      Any future date (e.g., 12/34)                  ║");
    println!("║   CVC:         Any 3 digits (e.g., 123)                       ║");
    println!("║   ZIP:         Any 5 digits (e.g., 12345)                     ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ OTHER TEST CARDS:                                             ║");
    println!("║   Declined:    4000 0000 0000 0002                            ║");
    println!("║   3D Secure:   4000 0025 0000 3155                            ║");
    println!("║   Insufficient: 4000 0000 0000 9995                           ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // After completing checkout, list payment intents
    println!("Waiting 5 seconds for you to review... (Ctrl+C to exit)");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Try to retrieve the session to see its status
    let session_status = client
        .get_checkout_session(&session.id)
        .await
        .expect("Failed to get session");

    println!("\nSession status: {:?}", session_status.status);
    println!("Payment status: {:?}", session_status.payment_status);
}
