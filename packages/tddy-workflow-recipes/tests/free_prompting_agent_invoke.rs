//! Free-prompting must drive the real agent path: the `prompting` graph step invokes [`CodingBackend`].
//! Echo-only tasks are for isolated workflow tests, not the free-prompting recipe graph.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::backend::{CodingBackend, InvokeRequest, InvokeResponse};
use tddy_core::workflow::context::Context;
use tddy_core::{BackendError, StubBackend, WorkflowRecipe};
use tddy_workflow_recipes::FreePromptingRecipe;

struct RecordingStubBackend {
    inner: Arc<StubBackend>,
    invoke_count: Arc<AtomicU32>,
}

#[async_trait]
impl CodingBackend for RecordingStubBackend {
    fn submit_channel(&self) -> Option<&tddy_core::toolcall::SubmitResultChannel> {
        self.inner.submit_channel()
    }

    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        self.invoke_count.fetch_add(1, Ordering::SeqCst);
        self.inner.invoke(request).await
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}

#[tokio::test]
async fn free_prompting_prompting_task_invokes_coding_backend() {
    let recipe = FreePromptingRecipe;
    let inner = Arc::new(StubBackend::new());
    let invoke_count = Arc::new(AtomicU32::new(0));
    let backend: Arc<dyn CodingBackend> = Arc::new(RecordingStubBackend {
        inner: inner.clone(),
        invoke_count: invoke_count.clone(),
    });

    let graph = recipe.build_graph(backend);
    let task = graph
        .get_task("prompting")
        .expect("free-prompting graph must expose a prompting task");

    let ctx = Context::new();
    ctx.set_sync("feature_input", "hello SKIP_QUESTIONS");
    ctx.set_sync("output_dir", std::env::temp_dir());
    ctx.set_sync("session_id", "test-session-free-prompting");
    ctx.set_sync("is_resume", false);

    task.run(ctx)
        .await
        .expect("prompting task should complete without workflow error");

    let n = invoke_count.load(Ordering::SeqCst);
    assert!(
        n >= 1,
        "free-prompting prompting step must call CodingBackend::invoke for agent interaction; got {} invoke(s)",
        n
    );
}
