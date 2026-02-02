//! Health endpoint integration tests.

mod common;

use common::TestHarness;

#[tokio::test]
async fn health_check_returns_ok() {
    let harness = TestHarness::new();

    let response = harness.server.get("/health").await;

    response.assert_status_ok();
}

#[tokio::test]
async fn health_check_returns_json() {
    let harness = TestHarness::new();

    let response = harness.server.get("/health").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["status"], "ok");
}
