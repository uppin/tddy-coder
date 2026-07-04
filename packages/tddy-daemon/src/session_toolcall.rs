//! Per-session toolcall listener + managed-workflow wiring for claude-cli sessions.
//!
//! A managed claude-cli session is *workflow-aware*: the daemon builds a [`WorkflowController`]
//! positioned at the recipe's start goal, and hosts a **per-session** toolcall listener whose
//! `transition` handler is that controller. When the agent runs `tddy-tools transition --to <goal>`
//! on the host, the `TDDY_SOCKET` relay reaches this listener and advances the controller, which
//! persists the new state into the session's `changeset.yaml`.
//!
//! The handler is bound **per instance** (see [`tddy_core::toolcall::ToolcallRpcService::with_transition_handler`])
//! so concurrent managed sessions never route a `transition` to another session's controller. Only
//! the `transition` verb is meaningful over this listener; `ask`/`approve` for claude-cli sessions
//! go through the MCP `approval_prompt` tool + PTY, not `TDDY_SOCKET`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use tddy_core::backend::{CodingBackend, GoalId, WorkflowRecipe};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::toolcall::{
    ChildSpawnHandler, ToolCallRequest, ToolcallRpcService, TransitionHandler,
};
use tddy_core::workflow::controller::WorkflowController;
use tddy_core::StubBackend;
use tokio::net::UnixListener;
use tokio::task::JoinHandle;

/// A running per-session toolcall listener. The bound socket is removed and the accept task aborted
/// on drop, so the listener's lifetime is tied to the owning session's state.
pub struct SessionToolcallListener {
    socket_path: PathBuf,
    accept_task: JoinHandle<()>,
    /// Held so the per-connection `ToolcallRpcService` senders never observe a disconnected
    /// receiver. Nothing drains it — claude-cli managed sessions use this listener for `transition`
    /// only (which never touches this channel). Wrapped in a `Mutex` to stay `Sync`.
    _requests: Mutex<Receiver<ToolCallRequest>>,
}

impl SessionToolcallListener {
    /// Path of the bound socket — pass to the session's process as `TDDY_SOCKET`.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for SessionToolcallListener {
    fn drop(&mut self) {
        self.accept_task.abort();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// The workflow wiring for one managed claude-cli session.
pub struct ManagedWorkflow {
    /// The state machine the session's `transition` calls drive; persists to `changeset.yaml`.
    pub controller: Arc<WorkflowController>,
    /// The per-session toolcall listener whose handler is `controller`.
    pub listener: SessionToolcallListener,
    /// The controller's initial position: the recipe's start goal for a new session (seed the
    /// changeset with this before launch), or the persisted resume goal for a resumed session.
    pub start_goal: GoalId,
    /// The recipe's orchestration system prompt to inject into the launched agent.
    pub orchestration_prompt: String,
    /// Drains the controller's workflow-event channel. State is persisted to `changeset.yaml`;
    /// live event streaming to the web client is a follow-up (see the feature PRD non-goals).
    pub event_drain: std::thread::JoinHandle<()>,
}

/// Bind a per-session toolcall listener whose `transition` handler is `controller`.
///
/// `socket_dir` must be a short directory (the AF_UNIX path is bound on the host and must satisfy
/// the platform's `SUN_LEN` limit — the session dir is too deep, so callers pass a short location).
#[allow(clippy::too_many_arguments)]
pub fn start_session_toolcall_listener(
    session_id: &str,
    socket_dir: &Path,
    session_dir: PathBuf,
    worktree_root: PathBuf,
    tddy_data_dir: PathBuf,
    controller: Arc<WorkflowController>,
    child_spawn_handler: Option<Arc<dyn ChildSpawnHandler>>,
) -> std::io::Result<SessionToolcallListener> {
    let socket_path = socket_dir.join(format!("tddy-wf-{session_id}.sock"));
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;

    let (tx, rx) = std::sync::mpsc::sync_channel::<ToolCallRequest>(32);
    let session_dir = Arc::new(Some(session_dir));
    let repo_root = Arc::new(Some(worktree_root));
    let tddy_data_dir = Arc::new(tddy_data_dir);
    let handler: Arc<dyn TransitionHandler> = controller;

    let accept_task = tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let service = ToolcallRpcService::with_transition_handler(
                tx.clone(),
                Arc::clone(&session_dir),
                Arc::clone(&repo_root),
                Arc::clone(&tddy_data_dir),
                Some(Arc::clone(&handler)),
            )
            .with_child_spawn_handler(child_spawn_handler.clone());
            let (reader, writer) = stream.into_split();
            let (_client, endpoint) =
                tddy_stdio::StdioEndpoint::from_duplex(reader, writer, service);
            tokio::spawn(endpoint.run());
        }
    });

    Ok(SessionToolcallListener {
        socket_path,
        accept_task,
        _requests: Mutex::new(rx),
    })
}

/// Build the managed-workflow wiring for a **new** claude-cli session: a controller positioned at
/// the recipe's start goal, the per-session toolcall listener bound to it, the recipe's orchestration
/// prompt, and an event drain. Does **not** seed `changeset.yaml` — the caller writes the changeset
/// (seeding the start goal) before/after this so the controller can persist transitions into it.
#[allow(clippy::too_many_arguments)]
pub fn set_up_managed_workflow(
    session_id: &str,
    recipe: Arc<dyn WorkflowRecipe>,
    session_dir: &Path,
    worktree_root: &Path,
    tddy_data_dir: &Path,
    socket_dir: &Path,
    child_spawn_handler: Option<Arc<dyn ChildSpawnHandler>>,
) -> Result<ManagedWorkflow, String> {
    let start = recipe.start_goal();
    build_managed_workflow(
        session_id,
        recipe,
        session_dir,
        worktree_root,
        tddy_data_dir,
        socket_dir,
        start,
        child_spawn_handler,
    )
}

/// Rebuild the managed-workflow wiring for a **resumed** claude-cli session: identical to
/// [`set_up_managed_workflow`] except the controller resumes at `resume_at` (the goal persisted in
/// `changeset.yaml`) rather than the recipe's start goal, so `transition` validation continues from
/// the session's actual position instead of restarting the workflow.
#[allow(clippy::too_many_arguments)]
pub fn resume_managed_workflow(
    session_id: &str,
    recipe: Arc<dyn WorkflowRecipe>,
    session_dir: &Path,
    worktree_root: &Path,
    tddy_data_dir: &Path,
    socket_dir: &Path,
    resume_at: GoalId,
    child_spawn_handler: Option<Arc<dyn ChildSpawnHandler>>,
) -> Result<ManagedWorkflow, String> {
    build_managed_workflow(
        session_id,
        recipe,
        session_dir,
        worktree_root,
        tddy_data_dir,
        socket_dir,
        resume_at,
        child_spawn_handler,
    )
}

/// Shared builder for [`set_up_managed_workflow`] / [`resume_managed_workflow`]: positions the
/// controller at `start`, binds the per-session listener to it, and returns the wiring bundle.
#[allow(clippy::too_many_arguments)]
fn build_managed_workflow(
    session_id: &str,
    recipe: Arc<dyn WorkflowRecipe>,
    session_dir: &Path,
    worktree_root: &Path,
    tddy_data_dir: &Path,
    socket_dir: &Path,
    start: GoalId,
    child_spawn_handler: Option<Arc<dyn ChildSpawnHandler>>,
) -> Result<ManagedWorkflow, String> {
    // The controller only reads the graph's topology (`successors`); tasks are never executed, so a
    // stub backend is sufficient for graph construction.
    let backend: Arc<dyn CodingBackend> = Arc::new(StubBackend::new());
    let graph = Arc::new(recipe.build_graph(backend));

    let (event_tx, event_rx) = std::sync::mpsc::channel::<WorkflowEvent>();
    let controller = Arc::new(WorkflowController::new_at(
        recipe.clone(),
        graph,
        Some(session_dir.to_path_buf()),
        Some(event_tx),
        start.clone(),
    ));
    let orchestration_prompt = recipe.orchestration_system_prompt(&start);

    let listener = start_session_toolcall_listener(
        session_id,
        socket_dir,
        session_dir.to_path_buf(),
        worktree_root.to_path_buf(),
        tddy_data_dir.to_path_buf(),
        controller.clone(),
        child_spawn_handler,
    )
    .map_err(|e| format!("failed to start session toolcall listener: {e}"))?;

    let event_drain = std::thread::spawn(move || {
        // Drain until the controller (and its sender) drops. Durable state lives in changeset.yaml.
        while event_rx.recv().is_ok() {}
    });

    Ok(ManagedWorkflow {
        controller,
        listener,
        start_goal: start,
        orchestration_prompt,
        event_drain,
    })
}
