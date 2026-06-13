//! Acceptance tests: `BackendInvokeTask` populates `InvokeRequest.remote` from ctx keys.
//!
//! AC: when `WorkflowContext` contains `remote_daemon_url`, `remote_session_id`, and
//! `remote_session_token`, the `BackendInvokeTask` must pass a fully-populated `RemoteToolEnv`
//! in `InvokeRequest.remote` to the backend — not `None`.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::backend::{CodingBackend, InvokeRequest, InvokeResponse};
use tddy_core::error::BackendError;
use tddy_core::toolcall::SubmitResultChannel;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::task::{BackendInvokeTask, Task};
use tddy_core::GoalId;
use tddy_workflow_recipes::FreePromptingRecipe;

/// A backend that captures the first `InvokeRequest` it receives and immediately returns success.
/// Does not use a submit channel — task completes from invoke output alone.
struct CapturingBackend {
    captured: Arc<std::sync::Mutex<Option<InvokeRequest>>>,
}

impl CapturingBackend {
    fn new() -> (Self, Arc<std::sync::Mutex<Option<InvokeRequest>>>) {
        let captured = Arc::new(std::sync::Mutex::new(None));
        (Self { captured: Arc::clone(&captured) }, captured)
    }
}

#[async_trait]
impl CodingBackend for CapturingBackend {
    fn submit_channel(&self) -> Option<&SubmitResultChannel> {
        None
    }

    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        *self.captured.lock().unwrap() = Some(request);
        Ok(InvokeResponse {
            output: "captured".to_string(),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        })
    }

    fn name(&self) -> &str {
        "capturing-test"
    }
}

/// AC: `BackendInvokeTask` must populate `InvokeRequest.remote` from ctx keys.
///
/// When the WorkflowContext has `remote_daemon_url`, `remote_session_id`, and
/// `remote_session_token` set, the backend must receive `request.remote = Some(RemoteToolEnv {
/// daemon_url, session_id, session_token, ... })`.
#[tokio::test]
async fn backend_invoke_task_populates_remote_from_ctx_keys() {
    let (backend, captured) = CapturingBackend::new();
    let backend: Arc<dyn CodingBackend> = Arc::new(backend);

    let task = BackendInvokeTask::from_recipe(
        "remote-invoke",
        GoalId::new("prompting"),
        Arc::new(FreePromptingRecipe),
        backend,
    );

    let ctx = Context::new();
    ctx.set_sync("feature_input", "describe the remote codebase");
    ctx.set_sync("output_dir", std::env::temp_dir());
    ctx.set_sync("remote_daemon_url", "http://relay.local:9001".to_string());
    ctx.set_sync("remote_session_id", "sess-remote-abc".to_string());
    ctx.set_sync("remote_session_token", "tok-remote-xyz".to_string());

    // run() may or may not complete the workflow — we only need the first backend.invoke() call.
    let _ = task.run(ctx).await;

    let req = captured
        .lock()
        .unwrap()
        .take()
        .expect("backend must have been invoked at least once");

    assert!(
        req.remote.is_some(),
        "InvokeRequest.remote must be Some(RemoteToolEnv) when ctx has remote keys; got None"
    );

    let env = req.remote.unwrap();
    assert_eq!(
        env.daemon_url, "http://relay.local:9001",
        "remote.daemon_url must match ctx key"
    );
    assert_eq!(
        env.session_id, "sess-remote-abc",
        "remote.session_id must match ctx key"
    );
    assert_eq!(
        env.session_token, "tok-remote-xyz",
        "remote.session_token must match ctx key"
    );
}

/// AC: when ctx has no remote keys, `InvokeRequest.remote` must be `None`.
///
/// Verifies the `extract_remote_env_from_ctx` short-circuit: absent keys → no remote env.
#[tokio::test]
async fn backend_invoke_task_remote_is_none_when_ctx_keys_absent() {
    let (backend, captured) = CapturingBackend::new();
    let backend: Arc<dyn CodingBackend> = Arc::new(backend);

    let task = BackendInvokeTask::from_recipe(
        "local-invoke",
        GoalId::new("prompting"),
        Arc::new(FreePromptingRecipe),
        backend,
    );

    let ctx = Context::new();
    ctx.set_sync("feature_input", "describe the local codebase");
    ctx.set_sync("output_dir", std::env::temp_dir());
    // No remote_* keys.

    let _ = task.run(ctx).await;

    let req = captured
        .lock()
        .unwrap()
        .take()
        .expect("backend must have been invoked at least once");

    assert!(
        req.remote.is_none(),
        "InvokeRequest.remote must be None when no remote ctx keys are set; got Some"
    );
}
