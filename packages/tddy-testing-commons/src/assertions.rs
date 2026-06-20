//! Domain-specific assertion helpers.
//!
//! Extension traits return `&Self` so assertions chain into one fluent sentence.
//! Each method carries a message that pinpoints the failure.

use tddy_core::backend::InvokeResponse;
use tddy_core::error::BackendError;

// ─── InvokeResponse assertions ───────────────────────────────────────────────

/// Extension trait for asserting on [`InvokeResponse`].
pub trait InvokeResponseAssertions {
    /// Assert the exit code equals `expected`.
    fn assert_exit_code(&self, expected: i32) -> &Self;

    /// Assert exit code is 0.
    fn assert_successful(&self) -> &Self {
        self.assert_exit_code(0)
    }

    /// Assert `output` contains `fragment`.
    fn assert_output_contains(&self, fragment: &str) -> &Self;

    /// Assert `output` is exactly `expected`.
    fn assert_output(&self, expected: &str) -> &Self;
}

impl InvokeResponseAssertions for InvokeResponse {
    fn assert_exit_code(&self, expected: i32) -> &Self {
        assert_eq!(
            self.exit_code,
            expected,
            "expected exit code {}, was {}",
            expected,
            self.exit_code
        );
        self
    }

    fn assert_output_contains(&self, fragment: &str) -> &Self {
        assert!(
            self.output.contains(fragment),
            "expected output to contain {:?}\nactual output: {:?}",
            fragment,
            self.output
        );
        self
    }

    fn assert_output(&self, expected: &str) -> &Self {
        assert_eq!(self.output.as_str(), expected, "output mismatch");
        self
    }
}

// ─── BackendError / Result assertions ────────────────────────────────────────

/// Asserts that `result` is `Err(BackendError)` and returns a fluent assert builder.
///
/// Panics immediately if the result is `Ok`.
pub fn assert_backend_error<T>(result: Result<T, BackendError>) -> BackendErrorAssert {
    match result {
        Err(e) => BackendErrorAssert(e),
        Ok(_) => panic!("expected a BackendError but the call succeeded"),
    }
}

/// Fluent assertion builder for [`BackendError`].
pub struct BackendErrorAssert(BackendError);

impl BackendErrorAssert {
    /// Assert the error message contains `fragment`.
    pub fn has_message_containing(self, fragment: &str) -> Self {
        let msg = format!("{}", self.0);
        assert!(
            msg.contains(fragment),
            "expected BackendError message to contain {:?}\nactual: {:?}",
            fragment,
            msg
        );
        self
    }

    /// Assert the error is the `BinaryNotFound` variant.
    pub fn is_binary_not_found(self) -> Self {
        assert!(
            matches!(self.0, BackendError::BinaryNotFound(_)),
            "expected BinaryNotFound but got: {:?}",
            self.0
        );
        self
    }

    /// Assert the error is the `InvocationFailed` variant.
    pub fn is_invocation_failed(self) -> Self {
        assert!(
            matches!(self.0, BackendError::InvocationFailed(_)),
            "expected InvocationFailed but got: {:?}",
            self.0
        );
        self
    }
}
