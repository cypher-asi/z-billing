//! Common test utilities for z-billing integration tests.

#![allow(dead_code)] // Some utilities are used by different test files

use std::sync::Arc;

use axum::Router;
use axum_test::TestServer;
use tempfile::TempDir;

use z_billing_core::UserId;
use z_billing_service::{create_router, AppState, ServiceConfig};
use z_billing_store::RocksStore;

/// Test harness containing everything needed for integration tests.
pub struct TestHarness {
    /// The test server for making HTTP requests.
    pub server: TestServer,
    /// Temporary directory for the database (kept alive for test duration).
    pub _temp_dir: TempDir,
    /// A test user ID for authenticated requests.
    pub test_user_id: UserId,
    /// The service API key for service-to-service requests.
    pub service_api_key: String,
}

impl TestHarness {
    /// Create a new test harness with a fresh database.
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let store = RocksStore::open(temp_dir.path()).expect("Failed to open store");

        let service_api_key = "test-service-key".to_string();

        let config = ServiceConfig {
            listen_addr: "127.0.0.1:0".into(),
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            auth_base_url: "http://localhost".into(),
            auth_audience: "z-billing".into(),
            service_api_key: Some(service_api_key.clone()),
            lago_api_url: None,
            lago_api_key: None,
            lago_organization_id: None,
            stripe_api_key: None,
            stripe_webhook_secret: None,
            frontend_url: "http://localhost:3000".into(),
            cors_origins: vec!["*".into()],
            max_body_bytes: 1024 * 1024,
            request_timeout_seconds: 30,
            pricing: z_billing_core::PricingConfig::default(),
        };

        let state = AppState::new(Arc::new(store), config);
        let router: Router = create_router(state);

        let server = TestServer::new(router).expect("Failed to create test server");
        let test_user_id = UserId::generate();

        Self {
            server,
            _temp_dir: temp_dir,
            test_user_id,
            service_api_key,
        }
    }

    /// Get the authorization header for user authentication.
    pub fn user_auth_header(&self) -> String {
        format!("Bearer test-token:{}", self.test_user_id)
    }

    /// Get a different user's auth header (for testing isolation).
    pub fn other_user_auth_header() -> String {
        let other_user = UserId::generate();
        format!("Bearer test-token:{}", other_user)
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}
