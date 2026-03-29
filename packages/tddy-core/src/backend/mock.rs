//! Mock backend for testing.
//!
//! Stores response output via per-instance SubmitResultChannel for test isolation.
//! Workflow tasks read via submit_channel().take_for_goal() or take_submit_result_for_goal.

use super::{ClarificationQuestion, CodingBackend, InvokeRequest, InvokeResponse};
use crate::error::BackendError;
use crate::stream::ProgressEvent;
use crate::toolcall::SubmitResultChannel;
use std::collections::VecDeque;
use std::sync::RwLock;

#[derive(Debug)]
struct QueuedMockResponse {
    result: Result<InvokeResponse, BackendError>,
    /// When true, this invoke does not record output in `submit_channel` (no `tddy-tools submit`).
    suppress_submit_store: bool,
}

/// Mock backend that returns pre-configured responses for testing.
#[derive(Debug)]
pub struct MockBackend {
    responses: RwLock<VecDeque<QueuedMockResponse>>,
    invocations: RwLock<Vec<InvokeRequest>>,
    submit_channel: SubmitResultChannel,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBackend {
    /// Create a new empty mock backend.
    pub fn new() -> Self {
        Self {
            responses: RwLock::new(VecDeque::new()),
            invocations: RwLock::new(Vec::new()),
            submit_channel: SubmitResultChannel::new(),
        }
    }

    /// Push a response to be returned on the next invoke() call.
    pub fn push_response(&self, response: Result<InvokeResponse, BackendError>) {
        self.responses
            .write()
            .unwrap()
            .push_back(QueuedMockResponse {
                result: response,
                suppress_submit_store: false,
            });
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

    /// Like [`push_ok`](Self::push_ok), but this `invoke` does not record output as a submit
    /// result (no `tddy-tools submit` delivery).
    pub fn push_ok_without_submit(&self, output: impl Into<String>) {
        self.responses
            .write()
            .unwrap()
            .push_back(QueuedMockResponse {
                result: Ok(InvokeResponse {
                    output: output.into(),
                    exit_code: 0,
                    session_id: None,
                    questions: vec![],
                    raw_stream: None,
                    stderr: None,
                }),
                suppress_submit_store: true,
            });
    }

    /// Push an error response.
    pub fn push_err(&self, error: &str) {
        self.push_response(Err(BackendError::InvocationFailed(error.to_string())));
    }

    /// Push a successful response with the given output, session_id, and questions.
    /// Does not store in submit_channel (agent has not submitted yet).
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

        let QueuedMockResponse {
            result: response,
            suppress_submit_store: suppress_submit,
        } = self
            .responses
            .write()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| QueuedMockResponse {
                result: Err(BackendError::InvocationFailed("no mock response".into())),
                suppress_submit_store: false,
            });

        let response = response?;

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

        // Only store submit when the agent produced final output (no pending questions).
        // When returning questions, the agent has not called tddy-tools submit yet.
        if response.questions.is_empty() && !suppress_submit {
            self.submit_channel
                .store(request.submit_key.as_str(), &response.output);
        }

        if let Some(ref sink) = request.progress_sink {
            sink.emit(&ProgressEvent::AgentExited {
                exit_code: response.exit_code,
                goal: request.submit_key.to_string(),
            });
        }

        Ok(response)
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn submit_channel(&self) -> Option<&SubmitResultChannel> {
        Some(&self.submit_channel)
    }
}
