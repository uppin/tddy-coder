//! In-memory fakes and mock wrappers for test isolation.

// Re-export so tests have one import surface.
pub use tddy_core::backend::MockBackend;

/// Create a new [`MockBackend`] with no queued responses.
///
/// Call `.push_ok("…")` or `.push_response(…)` to enqueue responses before the test action.
pub fn mock_backend() -> MockBackend {
    MockBackend::new()
}

/// Ergonomic wrapper: create a [`MockBackend`] pre-loaded with one `Ok` response.
///
/// ```
/// use tddy_testing_commons::fakes::mock_backend_returning_ok;
/// let backend = mock_backend_returning_ok("{ \"output\": \"done\" }");
/// ```
pub fn mock_backend_returning_ok(output: impl Into<String>) -> MockBackend {
    let backend = MockBackend::new();
    backend.push_ok(output);
    backend
}

/// Ergonomic wrapper: create a [`MockBackend`] pre-loaded with a sequence of `Ok` responses.
pub fn mock_backend_returning_outputs(outputs: &[&str]) -> MockBackend {
    let backend = MockBackend::new();
    for &output in outputs {
        backend.push_ok(output);
    }
    backend
}
