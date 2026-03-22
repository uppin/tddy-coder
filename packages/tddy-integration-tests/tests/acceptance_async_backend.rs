//! Acceptance tests for Milestone 2: async CodingBackend.
//!
//! Verifies that CodingBackend::invoke is async and can be awaited.

mod common;

use tddy_core::{CodingBackend, MockBackend};

#[tokio::test]
async fn mock_backend_invoke_is_async() {
    let backend = MockBackend::new();
    backend.push_ok("test output");

    let req = common::stub_invoke_request("test", "plan");

    let result = backend.invoke(req).await;
    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(resp.output, "test output");
    assert_eq!(resp.exit_code, 0);
}
