//! Mock backend for testing.
//!
//! Stores response output via store_submit_result so workflow tasks can take_submit_result_for_goal.

use super::{ClarificationQuestion, CodingBackend, InvokeRequest, InvokeResponse};
use crate::error::BackendError;
use crate::toolcall::store_submit_result;
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
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }));
    }

    /// Push an error response.
    pub fn push_err(&self, error: &str) {
        self.push_response(Err(BackendError::InvocationFailed(error.to_string())));
    }

    /// Push a successful response with the given output, session_id, and questions.
    pub fn push_ok_with_questions(
        &self,
        output: impl Into<String>,
        session_id: impl Into<String>,
        questions: Vec<ClarificationQuestion>,
    ) {
        self.push_response(Ok(InvokeResponse {
            output: output.into(),
            exit_code: 0,
            session_id: Some(session_id.into()),
            questions,
            raw_stream: None,
            stderr: None,
        }));
    }

    /// Push a successful response with raw_stream (for conversation output tests).
    pub fn push_ok_with_raw_stream(
        &self,
        output: impl Into<String>,
        raw_stream: impl Into<String>,
    ) {
        self.push_response(Ok(InvokeResponse {
            output: output.into(),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: Some(raw_stream.into()),
            stderr: None,
        }));
    }

    /// Get all invocations recorded so far.
    pub fn invocations(&self) -> Vec<InvokeRequest> {
        self.invocations.read().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl CodingBackend for MockBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        self.invocations.write().unwrap().push(request.clone());

        let response = self
            .responses
            .write()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(BackendError::InvocationFailed("no mock response".into())))?;

        if let Some(ref path) = request.conversation_output_path {
            let bytes = response.raw_stream.as_deref().unwrap_or(&response.output);
            tokio::fs::write(path, bytes.as_bytes())
                .await
                .map_err(|e| {
                    BackendError::InvocationFailed(format!(
                        "failed to write conversation output to {}: {}",
                        path.display(),
                        e
                    ))
                })?;
        }

        store_submit_result(request.goal.submit_key(), &response.output);

        Ok(response)
    }

    fn name(&self) -> &str {
        "mock"
    }
}
