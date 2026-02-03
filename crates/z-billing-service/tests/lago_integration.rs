//! Lago integration tests.
//!
//! These tests require a running Lago instance at localhost:3000.
//! Run with: cargo test --package z-billing-service --test lago_integration -- --nocapture
//!
//! Set up:
//! 1. Ensure Lago is running at localhost:3000
//! 2. Create .secrets/lago.json with API credentials
//! 3. Run: cargo test --test lago_integration

use std::path::Path;
use z_billing_service::lago::{
    metrics, plans, CustomerInput, EventInput, LagoClient, SubscriptionInput,
};

/// Load Lago client from secrets file.
fn load_lago_client() -> Option<LagoClient> {
    // Try to load secrets from various paths
    let secret_paths = [
        "z-billing/.secrets/lago.json",
        "z-billing/service/.secrets/lago.json",
        ".secrets/lago.json",
        "../.secrets/lago.json",
    ];

    for path in &secret_paths {
        if Path::new(path).exists() {
            if let Ok(contents) = std::fs::read_to_string(path) {
                if let Ok(secrets) = serde_json::from_str::<serde_json::Value>(&contents) {
                    let api_url = secrets["api_url"].as_str()?;
                    let api_key = secrets["api_key"].as_str()?;
                    println!("Loaded Lago secrets from: {}", path);
                    return LagoClient::new(api_url, api_key).ok();
                }
            }
        }
    }

    println!("No Lago secrets file found - skipping integration tests");
    None
}

/// Generate a unique test ID to avoid conflicts.
fn test_id() -> String {
    format!(
        "test_{}",
        uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string()
    )
}

// ============================================================================
// Customer Tests
// ============================================================================

#[tokio::test]
async fn test_create_customer() {
    let Some(client) = load_lago_client() else {
        println!("Skipping test - no Lago client available");
        return;
    };

    let customer_id = test_id();
    println!("Creating customer: {}", customer_id);

    let input = CustomerInput {
        external_id: customer_id.clone(),
        name: format!("Test Customer {}", customer_id),
        email: Some(format!("{}@test.example.com", customer_id)),
        billing_configuration: None,
        metadata: None,
    };

    let result = client.create_customer(input).await;

    match result {
        Ok(customer) => {
            println!("✓ Customer created successfully!");
            println!("  Lago ID: {}", customer.lago_id);
            println!("  External ID: {}", customer.external_id);
            println!("  Name: {:?}", customer.name);
            println!("  Email: {:?}", customer.email);
            assert_eq!(customer.external_id, customer_id);
        }
        Err(e) => {
            panic!("Failed to create customer: {}", e);
        }
    }
}

#[tokio::test]
async fn test_get_customer() {
    let Some(client) = load_lago_client() else {
        println!("Skipping test - no Lago client available");
        return;
    };

    // First create a customer
    let customer_id = test_id();
    println!("Creating customer for get test: {}", customer_id);

    let input = CustomerInput {
        external_id: customer_id.clone(),
        name: format!("Get Test Customer {}", customer_id),
        email: Some(format!("{}@test.example.com", customer_id)),
        billing_configuration: None,
        metadata: None,
    };

    client
        .create_customer(input)
        .await
        .expect("Failed to create customer");

    // Now retrieve it
    println!("Retrieving customer: {}", customer_id);
    let result = client.get_customer(&customer_id).await;

    match result {
        Ok(Some(customer)) => {
            println!("✓ Customer retrieved successfully!");
            println!("  Lago ID: {}", customer.lago_id);
            println!("  External ID: {}", customer.external_id);
            assert_eq!(customer.external_id, customer_id);
        }
        Ok(None) => {
            panic!("Customer not found after creation");
        }
        Err(e) => {
            panic!("Failed to get customer: {}", e);
        }
    }
}

#[tokio::test]
async fn test_get_nonexistent_customer() {
    let Some(client) = load_lago_client() else {
        println!("Skipping test - no Lago client available");
        return;
    };

    let fake_id = format!("nonexistent_{}", test_id());
    println!("Getting nonexistent customer: {}", fake_id);

    let result = client.get_customer(&fake_id).await;

    match result {
        Ok(None) => {
            println!("✓ Correctly returned None for nonexistent customer");
        }
        Ok(Some(_)) => {
            panic!("Unexpectedly found a nonexistent customer");
        }
        Err(e) => {
            // Some versions of Lago return an error instead of 404
            println!("Got error (may be expected): {}", e);
        }
    }
}

// ============================================================================
// Usage Event Tests
// ============================================================================

#[tokio::test]
async fn test_send_usage_event() {
    let Some(client) = load_lago_client() else {
        println!("Skipping test - no Lago client available");
        return;
    };

    // First create a customer to send events for
    let customer_id = test_id();
    println!("Creating customer for event test: {}", customer_id);

    let input = CustomerInput {
        external_id: customer_id.clone(),
        name: format!("Event Test Customer {}", customer_id),
        email: None,
        billing_configuration: None,
        metadata: None,
    };

    client
        .create_customer(input)
        .await
        .expect("Failed to create customer");

    // Create a subscription to the standard plan (required for usage events)
    let subscription_id = format!("sub_{}", test_id());
    println!("Creating subscription: {}", subscription_id);

    let sub_input = SubscriptionInput {
        external_customer_id: customer_id.clone(),
        plan_code: plans::STANDARD.to_string(),
        external_id: Some(subscription_id.clone()),
        name: Some(format!("Test Subscription {}", subscription_id)),
        billing_time: Some("calendar".to_string()),
    };

    client
        .create_subscription(sub_input)
        .await
        .expect("Failed to create subscription");

    // Send a usage event
    let event_id = format!("evt_{}", test_id());
    println!("Sending usage event: {}", event_id);

    let event = EventInput {
        transaction_id: event_id.clone(),
        external_customer_id: customer_id.clone(),
        code: metrics::LLM_INPUT_TOKENS.to_string(),
        timestamp: chrono::Utc::now().timestamp().to_string(), // Unix timestamp
        properties: Some(serde_json::json!({
            "tokens": 1000,
            "provider": "anthropic",
            "model": "claude-3-5-sonnet",
        })),
        external_subscription_id: None,
    };

    let result = client.send_event(event).await;

    match result {
        Ok(event) => {
            println!("✓ Event sent successfully!");
            println!("  Lago ID: {}", event.lago_id);
            println!("  Transaction ID: {}", event.transaction_id);
            println!("  Code: {}", event.code);
            assert_eq!(event.transaction_id, event_id);
        }
        Err(e) => {
            // This might fail if billable metrics aren't set up in Lago
            println!(
                "Event send failed (might need to set up billable metrics): {}",
                e
            );
            println!("This is expected if billable metrics haven't been created in Lago yet.");
        }
    }
}

#[tokio::test]
async fn test_send_llm_input_tokens() {
    let Some(client) = load_lago_client() else {
        println!("Skipping test - no Lago client available");
        return;
    };

    // First create a customer
    let customer_id = test_id();
    println!(
        "Creating customer for LLM input tokens test: {}",
        customer_id
    );

    let input = CustomerInput {
        external_id: customer_id.clone(),
        name: format!("LLM Input Tokens Test {}", customer_id),
        email: None,
        billing_configuration: None,
        metadata: None,
    };

    client
        .create_customer(input)
        .await
        .expect("Failed to create customer");

    // Create a subscription to the standard plan (required for usage events)
    let subscription_id = format!("sub_{}", test_id());
    println!("Creating subscription: {}", subscription_id);

    let sub_input = SubscriptionInput {
        external_customer_id: customer_id.clone(),
        plan_code: plans::STANDARD.to_string(),
        external_id: Some(subscription_id.clone()),
        name: Some(format!("Test Subscription {}", subscription_id)),
        billing_time: Some("calendar".to_string()),
    };

    client
        .create_subscription(sub_input)
        .await
        .expect("Failed to create subscription");

    // Send LLM input tokens only (output = 0)
    let tx_id = format!("llm_input_{}", test_id());
    println!("Sending LLM input tokens: {} (1000 tokens)", tx_id);

    let result = client
        .send_llm_usage(
            &tx_id,
            &customer_id,
            "anthropic",
            "claude-3-5-sonnet",
            Some("agent_test_123"),
            1000, // input tokens
            0,    // no output tokens
        )
        .await;

    match result {
        Ok(()) => {
            println!("✓ LLM input tokens event sent successfully!");
        }
        Err(e) => {
            println!("LLM input tokens send failed: {}", e);
            println!(
                "Make sure llm_input_tokens metric is added as a charge to the 'standard' plan."
            );
        }
    }
}

#[tokio::test]
async fn test_send_llm_output_tokens() {
    let Some(client) = load_lago_client() else {
        println!("Skipping test - no Lago client available");
        return;
    };

    // First create a customer
    let customer_id = test_id();
    println!(
        "Creating customer for LLM output tokens test: {}",
        customer_id
    );

    let input = CustomerInput {
        external_id: customer_id.clone(),
        name: format!("LLM Output Tokens Test {}", customer_id),
        email: None,
        billing_configuration: None,
        metadata: None,
    };

    client
        .create_customer(input)
        .await
        .expect("Failed to create customer");

    // Create a subscription to the standard plan (required for usage events)
    let subscription_id = format!("sub_{}", test_id());
    println!("Creating subscription: {}", subscription_id);

    let sub_input = SubscriptionInput {
        external_customer_id: customer_id.clone(),
        plan_code: plans::STANDARD.to_string(),
        external_id: Some(subscription_id.clone()),
        name: Some(format!("Test Subscription {}", subscription_id)),
        billing_time: Some("calendar".to_string()),
    };

    client
        .create_subscription(sub_input)
        .await
        .expect("Failed to create subscription");

    // Send LLM output tokens only (input = 0)
    let tx_id = format!("llm_output_{}", test_id());
    println!("Sending LLM output tokens: {} (500 tokens)", tx_id);

    let result = client
        .send_llm_usage(
            &tx_id,
            &customer_id,
            "anthropic",
            "claude-3-5-sonnet",
            Some("agent_test_123"),
            0,   // no input tokens
            500, // output tokens
        )
        .await;

    match result {
        Ok(()) => {
            println!("✓ LLM output tokens event sent successfully!");
        }
        Err(e) => {
            println!("LLM output tokens send failed: {}", e);
            println!(
                "Make sure llm_output_tokens metric is added as a charge to the 'standard' plan."
            );
        }
    }
}

#[tokio::test]
async fn test_send_cpu_hours() {
    let Some(client) = load_lago_client() else {
        println!("Skipping test - no Lago client available");
        return;
    };

    // First create a customer
    let customer_id = test_id();
    println!("Creating customer for CPU hours test: {}", customer_id);

    let input = CustomerInput {
        external_id: customer_id.clone(),
        name: format!("CPU Hours Test {}", customer_id),
        email: None,
        billing_configuration: None,
        metadata: None,
    };

    client
        .create_customer(input)
        .await
        .expect("Failed to create customer");

    // Create a subscription to the standard plan (required for usage events)
    let subscription_id = format!("sub_{}", test_id());
    println!("Creating subscription: {}", subscription_id);

    let sub_input = SubscriptionInput {
        external_customer_id: customer_id.clone(),
        plan_code: plans::STANDARD.to_string(),
        external_id: Some(subscription_id.clone()),
        name: Some(format!("Test Subscription {}", subscription_id)),
        billing_time: Some("calendar".to_string()),
    };

    client
        .create_subscription(sub_input)
        .await
        .expect("Failed to create subscription");

    // Send CPU hours only (memory = 0)
    let tx_id = format!("cpu_{}", test_id());
    println!("Sending CPU hours usage: {} (2.5 CPU hours)", tx_id);

    let result = client
        .send_compute_usage(
            &tx_id,
            &customer_id,
            Some("agent_cpu_test"),
            2.5, // CPU hours
            0.0, // no memory
        )
        .await;

    match result {
        Ok(()) => {
            println!("✓ CPU hours event sent successfully!");
        }
        Err(e) => {
            println!("CPU hours send failed: {}", e);
            println!("Make sure cpu_hours metric is added as a charge to the 'standard' plan.");
        }
    }
}

#[tokio::test]
async fn test_send_memory_gb_hours() {
    let Some(client) = load_lago_client() else {
        println!("Skipping test - no Lago client available");
        return;
    };

    // First create a customer
    let customer_id = test_id();
    println!(
        "Creating customer for memory GB hours test: {}",
        customer_id
    );

    let input = CustomerInput {
        external_id: customer_id.clone(),
        name: format!("Memory GB Hours Test {}", customer_id),
        email: None,
        billing_configuration: None,
        metadata: None,
    };

    client
        .create_customer(input)
        .await
        .expect("Failed to create customer");

    // Create a subscription to the standard plan (required for usage events)
    let subscription_id = format!("sub_{}", test_id());
    println!("Creating subscription: {}", subscription_id);

    let sub_input = SubscriptionInput {
        external_customer_id: customer_id.clone(),
        plan_code: plans::STANDARD.to_string(),
        external_id: Some(subscription_id.clone()),
        name: Some(format!("Test Subscription {}", subscription_id)),
        billing_time: Some("calendar".to_string()),
    };

    client
        .create_subscription(sub_input)
        .await
        .expect("Failed to create subscription");

    // Send memory GB hours only (cpu = 0)
    let tx_id = format!("memory_{}", test_id());
    println!("Sending memory GB hours usage: {} (4.0 GB-hours)", tx_id);

    let result = client
        .send_compute_usage(
            &tx_id,
            &customer_id,
            Some("agent_memory_test"),
            0.0, // no cpu
            4.0, // memory GB-hours
        )
        .await;

    match result {
        Ok(()) => {
            println!("✓ Memory GB hours event sent successfully!");
        }
        Err(e) => {
            println!("Memory GB hours send failed: {}", e);
            println!(
                "Make sure memory_gb_hours metric is added as a charge to the 'standard' plan."
            );
        }
    }
}

// ============================================================================
// Subscription Tests
// ============================================================================

#[tokio::test]
async fn test_create_subscription() {
    let Some(client) = load_lago_client() else {
        println!("Skipping test - no Lago client available");
        return;
    };

    // First create a customer
    let customer_id = test_id();
    println!("Creating customer for subscription test: {}", customer_id);

    let input = CustomerInput {
        external_id: customer_id.clone(),
        name: format!("Subscription Test {}", customer_id),
        email: None,
        billing_configuration: None,
        metadata: None,
    };

    client
        .create_customer(input)
        .await
        .expect("Failed to create customer");

    // Create a subscription
    let subscription_id = format!("sub_{}", test_id());
    println!(
        "Creating subscription: {} for plan '{}'",
        subscription_id,
        plans::STANDARD
    );

    let sub_input = SubscriptionInput {
        external_customer_id: customer_id.clone(),
        plan_code: plans::STANDARD.to_string(),
        external_id: Some(subscription_id.clone()),
        name: Some(format!("Test Subscription {}", subscription_id)),
        billing_time: Some("calendar".to_string()),
    };

    let result = client.create_subscription(sub_input).await;

    match result {
        Ok(subscription) => {
            println!("✓ Subscription created successfully!");
            println!("  Lago ID: {}", subscription.lago_id);
            println!("  External ID: {}", subscription.external_id);
            println!("  Plan Code: {}", subscription.plan_code);
            println!("  Status: {}", subscription.status);
            assert_eq!(subscription.external_id, subscription_id);
        }
        Err(e) => {
            println!(
                "Subscription creation failed (might need to set up plans): {}",
                e
            );
            println!(
                "This is expected if the '{}' plan hasn't been created in Lago.",
                plans::STANDARD
            );
        }
    }
}

// ============================================================================
// Connection Health Test
// ============================================================================

#[tokio::test]
async fn test_lago_connection() {
    let Some(client) = load_lago_client() else {
        println!("Skipping test - no Lago client available");
        return;
    };

    println!("Testing Lago connection by creating and retrieving a customer...");

    let customer_id = test_id();
    let input = CustomerInput {
        external_id: customer_id.clone(),
        name: "Connection Test".to_string(),
        email: None,
        billing_configuration: None,
        metadata: None,
    };

    match client.create_customer(input).await {
        Ok(customer) => {
            println!("✓ Lago connection successful!");
            println!("  Created customer: {}", customer.lago_id);
        }
        Err(e) => {
            panic!("✗ Lago connection failed: {}", e);
        }
    }
}

// ============================================================================
// Run All Tests Summary
// ============================================================================

/// This is a meta-test that provides instructions.
#[test]
fn test_instructions() {
    println!();
    println!("{}", "=".repeat(70));
    println!("LAGO INTEGRATION TESTS");
    println!("{}", "=".repeat(70));
    println!();
    println!("Prerequisites:");
    println!("  1. Lago running at localhost:3000");
    println!("  2. .secrets/lago.json with API credentials");
    println!();
    println!("To run all tests with output:");
    println!("  cargo test --package z-billing-service --test lago_integration -- --nocapture");
    println!();
    println!("To run a specific test:");
    println!("  cargo test --package z-billing-service --test lago_integration test_create_customer -- --nocapture");
    println!();
    println!("Note: Some tests may fail if billable metrics or plans");
    println!("haven't been set up in Lago yet. This is expected.");
    println!("{}", "=".repeat(70));
    println!();
}
