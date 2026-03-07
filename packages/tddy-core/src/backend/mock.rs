//! Mock backend for testing.

use super::{CodingBackend, InvokeRequest, InvokeResponse};
use crate::error::BackendError;
use std::collections::VecDeque;
use std::sync::RwLock;

/// Mock backend that returns pre-configured responses for testing.
#[derive(Debug, Default)]
pub struct MockBackend {
    responses: RwLock<VecDeque<Result<InvokeResponse, BackendError>>>,
    invocations: RwLock<Vec<InvokeRequest>>,
}

impl MockBackend {
    /// Create a new empty mock backend.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a response to be returned on the next invoke() call.
    pub fn push_response(&self, response: Result<InvokeResponse, BackendError>) {
        self.responses.write().unwrap().push_back(response);
    }

    /// Push a successful response with the given output.
    pub fn push_ok(&self, output: impl Into<String>) {
        self.push_response(Ok(InvokeResponse {
            output: output.into(),
            exit_code: 0,
        }));
    }

    /// Push an error response.
    pub fn push_err(&self, error: &str) {
        self.push_response(Err(BackendError::InvocationFailed(error.to_string())));
    }

    /// Get all invocations recorded so far.
    pub fn invocations(&self) -> Vec<InvokeRequest> {
        self.invocations.read().unwrap().clone()
    }
}

impl CodingBackend for MockBackend {
    fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        self.invocations.write().unwrap().push(request.clone());

        self.responses
            .write()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(BackendError::InvocationFailed("no mock response".into())))
    }
}
