//! ConnectionService implementation for daemon session/tool management.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use futures_util::stream::{Stream, StreamExt};
use livekit::prelude::Room;
use prost::Message as _;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::read_session_metadata;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_core::{BranchWorktreeIntent, Changeset, ChangesetWorkflow};
use tddy_rpc::{Request, Response, Status, Streaming};
use tddy_service::proto::connection::{
    AgentInfo, ClaimTerminalControlRequest, ClaimTerminalControlResponse, ConnectSessionRequest,
    ConnectSessionResponse, ConnectionService as ConnectionServiceTrait, CreateProjectRequest,
    CreateProjectResponse, DeleteSessionRequest, DeleteSessionResponse, EligibleDaemonEntry,
    ListAgentsRequest, ListAgentsResponse, ListEligibleDaemonsRequest, ListEligibleDaemonsResponse,
    ListProjectBranchesRequest, ListProjectBranchesResponse, ListProjectsRequest,
    ListProjectsResponse, ListSessionWorkflowFilesRequest, ListSessionWorkflowFilesResponse,
    ListSessionsRequest, ListSessionsResponse, ListSubagentsRequest, ListSubagentsResponse,
    ListTerminalSessionsRequest, ListTerminalSessionsResponse, ListToolsRequest, ListToolsResponse,
    ListWorktreesForProjectRequest, ListWorktreesForProjectResponse,
    ProjectEntry as ProtoProjectEntry, ReadSessionWorkflowFileRequest,
    ReadSessionWorkflowFileResponse, RemoveWorktreeRequest, RemoveWorktreeResponse,
    ReportSessionStatusRequest, ReportSessionStatusResponse, ResumeSessionRequest,
    ResumeSessionResponse, SendTerminalInputResponse, SessionEntry as ProtoSessionEntry,
    SessionTerminalInput, SessionTerminalOutput, Signal, SignalSessionRequest,
    SignalSessionResponse, StartSessionRequest, StartSessionResponse, StartTerminalSessionRequest,
    StartTerminalSessionResponse, StopTerminalSessionRequest, StopTerminalSessionResponse,
    StreamTerminalOutputRequest, SubagentInfo, TerminalControlEvent, TerminalSessionInfo, ToolInfo,
    WatchTerminalControlRequest, WorkflowFileEntry, WorktreeRow,
};
use uuid::Uuid;

use crate::agent_list_mapping::agent_allowlist_rows;
use crate::claude_cli_session::{ClaimOutcome, ClaudeCliSessionManager, MAIN_TERMINAL_ID};
use crate::config::DaemonConfig;
use crate::livekit_peer_discovery::{local_instance_id_for_config, LiveKitDiscoveryHandles};
use crate::multi_host::{EligibleDaemonSource, StubEligibleDaemonSource};
use crate::project_storage::{self, ProjectData};
use crate::session_deletion;
use crate::session_list_enrichment;
use crate::session_reader;
use crate::spawn_worker;
use crate::spawner::{self, SpawnOptions};
use crate::telegram_session_subscriber::TelegramDaemonHooks;
use crate::tool_catalog;
use crate::tool_engine;
use crate::user_sessions_path::{
    project_path_under_home_from_user_relative, projects_path_for_user, repos_base_for_user,
};
use crate::workspace_session;
use crate::worktrees::{self, RemoveWorktreeError, WorktreeStatsCache};
use tddy_service::proto::connection::{
    DemoVmState, ExecuteToolRequest, ExecuteToolResponse, GetDemoVmStatusRequest,
    GetDemoVmStatusResponse, ListExecToolsRequest, ListExecToolsResponse,
    ListSessionToolCallsRequest, ListSessionToolCallsResponse, StartDemoVmRequest,
    StartDemoVmResponse, StopDemoVmRequest, StopDemoVmResponse, ToolCallInfo as ProtoToolCallInfo,
};
use tddy_task::TaskRegistry;

/// Runs blocking clone/spawn work with a wall-clock cap so hung NSS/git/spawn cannot block RPCs forever.
async fn spawn_blocking_with_timeout<T: Send + 'static>(
    timeout: Duration,
    op_label: &'static str,
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<T, Status> {
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(Ok(v))) => Ok(v),
        Ok(Ok(Err(e))) => {
            log::error!("{} failed: {}", op_label, e);
            Err(Status::internal(e.to_string()))
        }
        Ok(Err(join_err)) => Err(Status::internal(join_err.to_string())),
        Err(_elapsed) => {
            log::error!(
                "{} timed out after {}s (spawn_worker_request_timeout_secs); blocking task may still run in the pool",
                op_label,
                timeout.as_secs()
            );
            Err(Status::deadline_exceeded(format!(
                "{}: timed out after {}s (see daemon log: spawner: child I/O paths; if same_user=false, parent blocks until pre_exec/initgroups completes)",
                op_label,
                timeout.as_secs()
            )))
        }
    }
}

/// Resolves session token to GitHub user login.
pub type SessionUserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Resolves OS user to sessions base path.
pub type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// Resolve a request's `terminal_id`, defaulting an empty value to the reserved main terminal so
/// existing single-terminal clients keep working.
fn resolved_terminal_id(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        MAIN_TERMINAL_ID
    } else {
        trimmed
    }
}

/// Derives the agent and recipe to relaunch a resumed session with, from its persisted
/// `.session.yaml`. Empty/whitespace-only values are treated as absent (`None`), mirroring the
/// spawner's trimming, so a legacy session with no persisted agent/recipe restores as `None`.
pub(crate) fn resume_agent_and_recipe(
    metadata: &tddy_core::SessionMetadata,
) -> (Option<String>, Option<String>) {
    fn non_blank(value: &Option<String>) -> Option<String> {
        value
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }
    (non_blank(&metadata.agent), non_blank(&metadata.recipe))
}

/// Stream adapter that yields [`SessionTerminalOutput`] from a broadcast receiver.
///
/// Implements [`futures_util::stream::Stream`] so it can be returned from
/// [`ConnectionServiceTrait::stream_session_terminal_io`].
pub struct TerminalOutputStream {
    rx: tokio::sync::broadcast::Receiver<bytes::Bytes>,
}

impl Stream for TerminalOutputStream {
    type Item = Result<SessionTerminalOutput, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use tokio::sync::broadcast::error::TryRecvError;
        loop {
            match self.rx.try_recv() {
                Ok(chunk) => {
                    return std::task::Poll::Ready(Some(Ok(SessionTerminalOutput {
                        data: chunk.to_vec(),
                    })));
                }
                Err(TryRecvError::Lagged(_)) => {
                    // Skip lagged messages and try again.
                    continue;
                }
                Err(TryRecvError::Closed) => {
                    return std::task::Poll::Ready(None);
                }
                Err(TryRecvError::Empty) => {
                    // Register the waker with a new future so we get notified when data arrives.
                    let mut rx_clone = self.rx.resubscribe();
                    let waker = cx.waker().clone();
                    tokio::spawn(async move {
                        // Wait for the next message, then wake the task.
                        let _ = rx_clone.recv().await;
                        waker.wake();
                    });
                    return std::task::Poll::Pending;
                }
            }
        }
    }
}

impl Unpin for TerminalOutputStream {}

/// Stream adapter backed by an mpsc channel — used for `StreamTerminalOutput` (browser-compatible
/// server-streaming RPC).
///
/// Unlike `TerminalOutputStream` (broadcast-based), this correctly registers the waker via
/// `poll_recv` so the stream is woken as soon as data arrives. A background task bridges the
/// broadcast channel into the mpsc sender so no messages can be lost between `try_recv()` and
/// waker registration.
pub struct MpscTerminalOutputStream {
    rx: tokio::sync::mpsc::UnboundedReceiver<bytes::Bytes>,
}

impl Stream for MpscTerminalOutputStream {
    type Item = Result<SessionTerminalOutput, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.rx.poll_recv(cx) {
            std::task::Poll::Ready(Some(chunk)) => {
                std::task::Poll::Ready(Some(Ok(SessionTerminalOutput {
                    data: chunk.to_vec(),
                })))
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl Unpin for MpscTerminalOutputStream {}

/// Stream adapter backed by an mpsc channel for [`TerminalControlEvent`] server-streaming.
pub struct MpscControlEventStream {
    rx: tokio::sync::mpsc::UnboundedReceiver<TerminalControlEvent>,
}

impl Stream for MpscControlEventStream {
    type Item = Result<TerminalControlEvent, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.rx.poll_recv(cx) {
            std::task::Poll::Ready(Some(event)) => std::task::Poll::Ready(Some(Ok(event))),
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl Unpin for MpscControlEventStream {}

/// Relay task for `WatchTerminalControl`: forwards `ControlChangeEvent` broadcasts scoped to
/// `session_id` as `TerminalControlEvent` messages into `tx`, computing `you_are_controller`
/// by re-validating the watcher's stored `control_token` on each change.
async fn relay_control_events(
    session_id: String,
    control_token: String,
    manager: Arc<crate::claude_cli_session::ClaudeCliSessionManager>,
    mut broadcast_rx: tokio::sync::broadcast::Receiver<
        crate::claude_cli_session::ControlChangeEvent,
    >,
    tx: tokio::sync::mpsc::UnboundedSender<TerminalControlEvent>,
) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match broadcast_rx.recv().await {
            Ok(change) if change.session_id == session_id => {
                let you = manager.verify_control(&session_id, &control_token).await;
                let event = TerminalControlEvent {
                    holder_screen_id: change.holder_screen_id,
                    you_are_controller: you,
                };
                if tx.send(event).is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

/// Per-session QEMU demo VM lifecycle state.
enum DemoVmHandle {
    /// Boot has been requested; waiting for SSH port to become reachable.
    Booting,
    /// VM is up and accepting SSH connections.
    /// `share_url` is the first app port forward URL (e.g. "http://localhost:8080"), if any.
    Running {
        vm: tddy_vm::RunningVm,
        share_url: String,
    },
    /// Boot or shutdown failed.
    Error(String),
}

/// ConnectionService implementation.
pub struct ConnectionServiceImpl {
    config: DaemonConfig,
    #[allow(dead_code)]
    // Kept for API compatibility; callers pass a resolver but tddy_data_dir is used directly.
    sessions_base_for_user: SessionsBaseResolver,
    tddy_data_dir: PathBuf,
    user_resolver: SessionUserResolver,
    spawn_client: Option<Arc<spawn_worker::SpawnClient>>,
    eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    /// When set, LiveKit **Room** handle for forwarding **StartSession** to peer daemons in `common_room`.
    common_room_livekit_room: Option<Arc<tokio::sync::RwLock<Option<Arc<Room>>>>>,
    telegram: Option<Arc<TelegramDaemonHooks>>,
    worktree_stats_cache: Arc<WorktreeStatsCache>,
    claude_cli_manager: Arc<ClaudeCliSessionManager>,
    /// Sandboxed claude-cli sessions (darwin Seatbelt).
    sandbox_manager: Arc<crate::sandbox_session::SandboxSessionManager>,
    /// Registry for Tasks created by tool invocations (every ExecuteTool call).
    task_registry: TaskRegistry,
    /// Optional idle-timeout tracker for relay mode — bumped on every RPC call.
    idle_tracker: Option<Arc<crate::relay_idle::IdleTimeoutTracker>>,
    /// Per-session demo VM state — keyed by session_id.
    demo_vm_state: Arc<tokio::sync::Mutex<std::collections::HashMap<String, DemoVmHandle>>>,
}

impl ConnectionServiceImpl {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: DaemonConfig,
        sessions_base_for_user: SessionsBaseResolver,
        tddy_data_dir: PathBuf,
        user_resolver: SessionUserResolver,
        spawn_client: Option<(spawn_worker::SpawnClient, i32)>,
        livekit_discovery: Option<LiveKitDiscoveryHandles>,
        telegram: Option<Arc<TelegramDaemonHooks>>,
        claude_cli_manager: Arc<ClaudeCliSessionManager>,
    ) -> Self {
        let spawn_client = spawn_client.map(|(c, _pid)| Arc::new(c));
        let (eligible_daemon_source, common_room_livekit_room) = match livekit_discovery {
            Some(h) => (h.eligible_daemon_source, Some(h.common_room_livekit_room)),
            None => (
                Arc::new(StubEligibleDaemonSource) as Arc<dyn EligibleDaemonSource>,
                None,
            ),
        };
        let worktree_stats_cache = Arc::new(WorktreeStatsCache::new(
            worktrees::projects_stats_cache_root(&tddy_data_dir),
        ));
        let task_registry = claude_cli_manager.task_registry();
        let demo_vm_state = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        Self {
            config,
            sessions_base_for_user,
            tddy_data_dir,
            user_resolver,
            spawn_client,
            eligible_daemon_source,
            common_room_livekit_room,
            telegram,
            worktree_stats_cache,
            claude_cli_manager,
            sandbox_manager: Arc::new(crate::sandbox_session::SandboxSessionManager::new()),
            task_registry,
            idle_tracker: None,
            demo_vm_state,
        }
    }

    /// Return the shared `TaskRegistry` so `main.rs` can pass it to other services.
    pub fn task_registry(&self) -> TaskRegistry {
        self.task_registry.clone()
    }

    /// Attach an idle-timeout tracker to this service (builder pattern).
    ///
    /// When set, every RPC handler calls `tracker.record_activity()` so the relay daemon does
    /// not self-terminate while a client is actively using the service.
    pub fn with_idle_tracker(
        mut self,
        tracker: Arc<crate::relay_idle::IdleTimeoutTracker>,
    ) -> Self {
        self.idle_tracker = Some(tracker);
        self
    }

    /// Record RPC activity in the idle-timeout tracker, if one is attached.
    fn record_rpc_activity(&self) {
        if let Some(ref tracker) = self.idle_tracker {
            tracker.record_activity();
        }
    }

    fn maybe_spawn_telegram_observer(&self, session_id: &str, grpc_port: u16) {
        if let Some(ref tg) = self.telegram {
            tg.spawn_presenter_observer_task(session_id, grpc_port);
        }
    }

    fn resolve_chain_base_ref(
        sessions_base: &std::path::Path,
        stack_parent: Option<&str>,
        repo_root: &std::path::Path,
    ) -> Result<Option<String>, Status> {
        let Some(sp) = stack_parent else {
            return Ok(None);
        };
        tddy_core::resolve_chain_integration_base_ref_from_parent_session(
            sessions_base,
            sp,
            repo_root,
        )
        .map(Some)
        .map_err(|e| {
            Status::failed_precondition(format!("could not resolve stack parent branch: {e}"))
        })
    }

    /// Handle `StartSession` for `session_type = "claude-cli"` sessions.
    ///
    /// Requires a valid, registered project. Creates a real git worktree under the project's
    /// main repo (via `tddy_core::setup_worktree_for_session_with_optional_chain_base`), then
    /// spawns the `claude` binary in a PTY.
    ///
    /// `initial_prompt` — when non-empty, passed as a positional argument to `claude` so it
    /// receives the first user turn on startup (e.g. `claude "build feature X"`).
    #[allow(clippy::too_many_arguments)]
    async fn start_claude_cli_session(
        &self,
        os_user: &str,
        session_id: &str,
        sessions_base: PathBuf,
        model: &str,
        project_id: &str,
        branch_worktree_intent: &str,
        new_branch_name: &str,
        selected_integration_base_ref: &str,
        selected_branch_to_work_on: &str,
        initial_prompt: &str,
        permission_mode: &str,
        stack_parent: Option<&str>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        if model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for claude-cli sessions",
            ));
        }

        // Require a valid, registered project — claude-cli always runs in a real worktree.
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument(
                "project_id is required for claude-cli sessions",
            ));
        }
        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        if !repo_root.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        // Create session directory under sessions_base/sessions/<id>/.
        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

        // Build branch intent and write a minimal changeset so the worktree setup fn can read it.
        let short_id = &session_id[..8.min(session_id.len())];
        let (intent, resolved_new_branch, resolved_selected_branch) = match branch_worktree_intent
            .trim()
        {
            "new_branch_from_base" => {
                if new_branch_name.trim().is_empty() {
                    return Err(Status::invalid_argument(
                            "new_branch_name is required when branch_worktree_intent is new_branch_from_base",
                        ));
                }
                (
                    BranchWorktreeIntent::NewBranchFromBase,
                    Some(new_branch_name.trim().to_string()),
                    None,
                )
            }
            "work_on_selected_branch" => {
                if selected_branch_to_work_on.trim().is_empty() {
                    return Err(Status::invalid_argument(
                            "selected_branch_to_work_on is required when branch_worktree_intent is work_on_selected_branch",
                        ));
                }
                (
                    BranchWorktreeIntent::WorkOnSelectedBranch,
                    None,
                    Some(selected_branch_to_work_on.trim().to_string()),
                )
            }
            _ => {
                // Default: create a new branch from the integration base with a generated name.
                (
                    BranchWorktreeIntent::NewBranchFromBase,
                    Some(format!("claude-cli/{}", short_id)),
                    None,
                )
            }
        };

        let cs_workflow = ChangesetWorkflow {
            branch_worktree_intent: Some(intent),
            new_branch_name: resolved_new_branch,
            selected_integration_base_ref: if selected_integration_base_ref.trim().is_empty() {
                None
            } else {
                Some(selected_integration_base_ref.trim().to_string())
            },
            selected_branch_to_work_on: resolved_selected_branch,
            ..ChangesetWorkflow::default()
        };
        let cs = Changeset {
            workflow: Some(cs_workflow),
            orchestrator_session_id: stack_parent.map(str::to_string),
            ..Changeset::default()
        };
        tddy_core::write_changeset(&session_dir, &cs)
            .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;

        let chain_base_ref =
            Self::resolve_chain_base_ref(&sessions_base, stack_parent, &repo_root)?;

        // Create the real git worktree (blocking: involves git fetch + git worktree add).
        let repo_root_clone = repo_root.clone();
        let session_dir_clone = session_dir.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let worktree_path = spawn_blocking_with_timeout(
            timeout,
            "start_claude_cli_session: create worktree",
            move || {
                tddy_core::setup_worktree_for_session_with_optional_chain_base(
                    &repo_root_clone,
                    &session_dir_clone,
                    chain_base_ref.as_deref(),
                )
                .map_err(|e| anyhow::anyhow!("worktree setup failed: {}", e))
            },
        )
        .await?;

        let tddy_tools_path = crate::sandbox_session::resolve_tddy_tools_path(
            self.config
                .claude_cli
                .as_ref()
                .and_then(|c| c.tddy_tools_path.as_deref()),
        );

        // Resolve daemon URL: config → http://127.0.0.1:{web_port}.
        let daemon_url = self
            .config
            .claude_cli
            .as_ref()
            .and_then(|c| c.daemon_url.as_deref())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                let port = self.config.listen.web_port.unwrap_or(8899);
                format!("http://127.0.0.1:{port}")
            });

        // Generate a per-session hook token and write .claude/settings.local.json into the
        // worktree. Claude Code reads this file on startup and wires the six lifecycle hooks.
        // Write failure is warn-and-continue so it never blocks the session from starting.
        let hook_token = Uuid::new_v4().to_string();
        let hooks_settings =
            tddy_core::build_claude_hooks_settings(&tddy_core::HookCommandParams {
                tddy_tools_path: &tddy_tools_path,
                daemon_url: &daemon_url,
                session_id,
                os_user,
                hook_token: &hook_token,
            });
        let claude_dir = worktree_path.join(".claude");
        if let Err(e) = std::fs::create_dir_all(&claude_dir).and_then(|_| {
            serde_json::to_string_pretty(&hooks_settings)
                .map_err(|e| std::io::Error::other(e.to_string()))
                .and_then(|json| std::fs::write(claude_dir.join("settings.local.json"), json))
        }) {
            log::warn!(
                "session {session_id}: failed to write .claude/settings.local.json — hooks will not fire: {e}"
            );
        }

        // Spawn the claude CLI process in a PTY inside the real worktree.
        let binary_path = self
            .config
            .claude_cli
            .as_ref()
            .map(|c| c.binary_path.as_str())
            .unwrap_or("claude");

        let manager = Arc::clone(&self.claude_cli_manager);
        let session_id_owned = session_id.to_string();
        let model_owned = model.to_string();
        let binary_owned = binary_path.to_string();
        let worktree_clone = worktree_path.clone();

        let initial_prompt_opt = {
            let p = initial_prompt.trim();
            if p.is_empty() {
                None
            } else {
                Some(p.to_string())
            }
        };
        let permission_mode_opt = {
            let m = permission_mode.trim();
            if m.is_empty() {
                None
            } else {
                Some(m.to_string())
            }
        };
        let handle = manager
            .start(
                &session_id_owned,
                worktree_clone,
                &model_owned,
                &binary_owned,
                initial_prompt_opt.as_deref(),
                permission_mode_opt.as_deref(),
            )
            .await
            .map_err(|e| Status::internal(format!("failed to spawn claude-cli: {}", e)))?;

        let pid = handle.pid;

        // Write .session.yaml.
        let now = chrono::Utc::now().to_rfc3339();
        let meta = tddy_core::SessionMetadata {
            session_id: session_id.to_string(),
            project_id: project_id.to_string(),
            created_at: now.clone(),
            updated_at: now,
            status: "active".to_string(),
            repo_path: Some(worktree_path.to_string_lossy().to_string()),
            pid: Some(pid),
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some(model.to_string()),
            activity_status: None,
            hook_token: Some(hook_token),
            sandbox: None,
            agent: None,
            recipe: None,
            specialized_agents: Vec::new(),
        };
        tddy_core::write_session_metadata(&session_dir, &meta)
            .map_err(|e| Status::internal(format!("failed to write session metadata: {}", e)))?;

        // Optionally expose the PTY via a per-session LiveKit participant so that LiveKit
        // clients (web UI, pty-relay --livekit-url) can use the same bidi-stream path as
        // tool sessions. Falls back gracefully: if LiveKit is not configured the session is
        // still usable via the gRPC connectrpc endpoints.
        let (lk_room, lk_url, lk_server_identity) = if let Some(lk) =
            spawner::livekit_creds_from_config(&self.config)
        {
            let room_name =
                spawner::resolve_livekit_room_name(lk.common_room.as_deref(), session_id);
            let server_identity = spawner::livekit_server_identity_for_session(
                lk.daemon_instance_id.as_deref(),
                session_id,
            );
            match crate::claude_cli_session::spawn_livekit_bridge(
                Arc::clone(&handle),
                &lk.url,
                &room_name,
                &lk.api_key,
                &lk.api_secret,
                &server_identity,
            )
            .await
            {
                Ok(()) => {
                    log::info!(
                        target: "tddy_daemon::connection_service",
                        "claude-cli session {}: LiveKit bridge started (identity={})",
                        session_id,
                        server_identity
                    );
                    (room_name, lk.url.clone(), server_identity)
                }
                Err(e) => {
                    log::warn!(
                        target: "tddy_daemon::connection_service",
                        "claude-cli session {}: LiveKit bridge failed ({}); gRPC path still works",
                        session_id,
                        e
                    );
                    (String::new(), String::new(), String::new())
                }
            }
        } else {
            (String::new(), String::new(), String::new())
        };

        log::info!(
            target: "tddy_daemon::connection_service",
            "started claude-cli session {} pid={} worktree={} user={}",
            session_id,
            pid,
            worktree_path.display(),
            os_user
        );

        Ok(Response::new(StartSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: lk_room,
            livekit_url: lk_url,
            livekit_server_identity: lk_server_identity,
        }))
    }

    /// Resolve `specialized_agents` names against `<tddyhome>/agents` (+ builtins) into their
    /// full defs (see docs/ft/coder/specialized-subagents.md). An unresolvable name is a request
    /// error — the session is never started with a silently-dropped subagent. An empty input
    /// resolves to an empty output, not an error.
    fn resolve_specialized_agent_defs(
        &self,
        specialized_agents: &[String],
    ) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        if specialized_agents.is_empty() {
            return Ok(Vec::new());
        }
        let agents_dir = self.tddy_data_dir.join("agents");
        let resolved = tddy_discovery::agent_def::resolve_agent_defs(&agents_dir);
        let mut selected = Vec::with_capacity(specialized_agents.len());
        for name in specialized_agents {
            let def = resolved.iter().find(|d| &d.name == name).ok_or_else(|| {
                Status::invalid_argument(format!(
                    "specialized_agents: unknown subagent '{name}' (not a builtin and not \
                         found under <tddyhome>/agents)"
                ))
            })?;
            selected.push(def.clone());
        }
        Ok(selected)
    }

    /// Build the `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON` jail env pair for already-resolved
    /// specialized-agent defs (see [`Self::resolve_specialized_agent_defs`]). Empty input produces
    /// no env pairs.
    fn specialized_subagent_env(
        &self,
        defs: &[tddy_discovery::agent_def::SpecializedAgentDef],
    ) -> Result<Vec<(String, String)>, Status> {
        if defs.is_empty() {
            return Ok(Vec::new());
        }
        let names = defs
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let defs_json = serde_json::to_string(defs).map_err(|e| {
            Status::internal(format!("failed to serialize specialized agent defs: {e}"))
        })?;
        Ok(vec![
            ("TDDY_SUBAGENT".to_string(), names),
            ("TDDY_SUBAGENTS_JSON".to_string(), defs_json),
        ])
    }

    /// Handle `StartSession` for sandboxed `claude-cli` sessions (darwin Seatbelt, local gRPC).
    #[allow(clippy::too_many_arguments)]
    async fn start_sandboxed_claude_cli_session(
        &self,
        os_user: &str,
        session_id: &str,
        sessions_base: PathBuf,
        model: &str,
        project_id: &str,
        branch_worktree_intent: &str,
        new_branch_name: &str,
        selected_integration_base_ref: &str,
        selected_branch_to_work_on: &str,
        permission_mode: &str,
        stack_parent: Option<&str>,
        // Specialized subagents (see docs/ft/coder/specialized-subagents.md). This sandboxed path
        // already never mounts the repo (`mounts: vec![]` below, unconditionally) —
        // `managed_codebase` is accepted for request-shape/UI-intent clarity, not to toggle mount
        // behavior. Names resolve against `<tddyhome>/agents` (+ builtins) and are wired into the
        // jail env; all configuration (model, base_url, max_turns, replaces) comes exclusively
        // from the resolved def.
        _managed_codebase: bool,
        specialized_agents: &[String],
    ) -> Result<Response<StartSessionResponse>, Status> {
        if model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for claude-cli sessions",
            ));
        }
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument(
                "project_id is required for claude-cli sessions",
            ));
        }
        let specialized_defs = self.resolve_specialized_agent_defs(specialized_agents)?;

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        if !repo_root.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

        let short_id = &session_id[..8.min(session_id.len())];
        let (intent, resolved_new_branch, resolved_selected_branch) = match branch_worktree_intent
            .trim()
        {
            "new_branch_from_base" => {
                if new_branch_name.trim().is_empty() {
                    return Err(Status::invalid_argument(
                        "new_branch_name is required when branch_worktree_intent is new_branch_from_base",
                    ));
                }
                (
                    BranchWorktreeIntent::NewBranchFromBase,
                    Some(new_branch_name.trim().to_string()),
                    None,
                )
            }
            "work_on_selected_branch" => {
                if selected_branch_to_work_on.trim().is_empty() {
                    return Err(Status::invalid_argument(
                        "selected_branch_to_work_on is required when branch_worktree_intent is work_on_selected_branch",
                    ));
                }
                (
                    BranchWorktreeIntent::WorkOnSelectedBranch,
                    None,
                    Some(selected_branch_to_work_on.trim().to_string()),
                )
            }
            _ => (
                BranchWorktreeIntent::NewBranchFromBase,
                Some(format!("claude-cli/{short_id}")),
                None,
            ),
        };

        let cs_workflow = ChangesetWorkflow {
            branch_worktree_intent: Some(intent),
            new_branch_name: resolved_new_branch,
            selected_integration_base_ref: if selected_integration_base_ref.trim().is_empty() {
                None
            } else {
                Some(selected_integration_base_ref.trim().to_string())
            },
            selected_branch_to_work_on: resolved_selected_branch,
            ..ChangesetWorkflow::default()
        };
        let cs = Changeset {
            workflow: Some(cs_workflow),
            orchestrator_session_id: stack_parent.map(str::to_string),
            ..Changeset::default()
        };
        tddy_core::write_changeset(&session_dir, &cs)
            .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;

        let chain_base_ref =
            Self::resolve_chain_base_ref(&sessions_base, stack_parent, &repo_root)?;

        let repo_root_clone = repo_root.clone();
        let session_dir_clone = session_dir.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let worktree_path = spawn_blocking_with_timeout(
            timeout,
            "start_sandboxed_claude_cli_session: create worktree",
            move || {
                tddy_core::setup_worktree_for_session_with_optional_chain_base(
                    &repo_root_clone,
                    &session_dir_clone,
                    chain_base_ref.as_deref(),
                )
                .map_err(|e| anyhow::anyhow!("worktree setup failed: {e}"))
            },
        )
        .await?;

        let sandbox_root = session_dir.join("sandbox");
        let egress_dir = session_dir.join("egress");
        std::fs::create_dir_all(sandbox_root.join(".work").join("home"))
            .map_err(|e| Status::internal(format!("mkdir sandbox scratch: {e}")))?;
        std::fs::create_dir_all(sandbox_root.join(".work").join("tmp"))
            .map_err(|e| Status::internal(format!("mkdir sandbox tmp: {e}")))?;
        std::fs::create_dir_all(sandbox_root.join("context"))
            .map_err(|e| Status::internal(format!("mkdir sandbox context: {e}")))?;
        std::fs::create_dir_all(&egress_dir)
            .map_err(|e| Status::internal(format!("mkdir sandbox egress: {e}")))?;

        // Resolve to the real (symlink-free) paths now that the dirs exist. Seatbelt
        // evaluates file rules — including AF_UNIX socket bind — against the fully
        // resolved path, so the socket/marker paths the runner binds must match the
        // canonical paths baked into the SBPL profile. Session dirs live under TMPDIR,
        // which on macOS is reached via the /tmp -> /private/tmp symlink; without this
        // the tool-IPC socket bind fails with "Operation not permitted".
        let sandbox_root = std::fs::canonicalize(&sandbox_root).unwrap_or(sandbox_root);
        let egress_dir = std::fs::canonicalize(&egress_dir).unwrap_or(egress_dir);
        let scratch_dir = sandbox_root.join(".work");
        let scratch_home = scratch_dir.join("home");
        let scratch_tmp = scratch_dir.join("tmp");
        let context_dir = sandbox_root.join("context");

        let replacement_pairs = subagent_replacement_pairs(&specialized_defs);
        let replacement_refs: Vec<Vec<&str>> = replacement_pairs
            .iter()
            .map(|(_, tools)| tools.iter().map(String::as_str).collect())
            .collect();
        let replacements: Vec<tddy_sandbox::SubagentReplacement<'_>> = replacement_pairs
            .iter()
            .zip(replacement_refs.iter())
            .map(|((name, _), refs)| tddy_sandbox::SubagentReplacement {
                name,
                replaced: refs,
            })
            .collect();
        let ctx = crate::sandbox_session::prepare_context_dir_with_subagent(
            &worktree_path,
            &replacements,
        )
        .map_err(Status::internal)?;
        crate::sandbox_session::copy_dir_all(ctx.path(), &context_dir).map_err(Status::internal)?;

        let tddy_tools_path = crate::sandbox_session::resolve_tddy_tools_path(
            self.config
                .claude_cli
                .as_ref()
                .and_then(|c| c.tddy_tools_path.as_deref()),
        );

        let claude_binary_cfg = self
            .config
            .claude_cli
            .as_ref()
            .map(|c| c.binary_path.as_str())
            .unwrap_or("claude");
        // Canonicalize the binary paths the runner will exec: the SBPL allow-list is built
        // from the canonical (symlink-resolved) parent dirs, so a symlinked spelling (e.g. a
        // binary under /tmp -> /private/tmp) would be denied at exec time ("doesn't exist /
        // Operation not permitted"). A relative/PATH-resolved name (no '/') is left as-is.
        let canonicalize_exec = |p: &str| -> String {
            if p.contains('/') {
                std::fs::canonicalize(p)
                    .map(|c| c.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| p.to_string())
            } else {
                p.to_string()
            }
        };
        let tddy_tools_path = canonicalize_exec(&tddy_tools_path);
        let sandbox_runner_path =
            canonicalize_exec(&crate::sandbox_session::resolve_sandbox_runner_path());
        let claude_binary = canonicalize_exec(claude_binary_cfg);
        let claude_binary = claude_binary.as_str();

        // The tool-IPC AF_UNIX socket must fit within SUN_LEN (104 bytes on macOS); the
        // canonical session dir is far too deep, so use a short out-of-tree path that the
        // SBPL profile grants an explicit literal allow (see SandboxSpec::ipc_socket).
        let tool_ipc_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(session_id);
        let ready_marker = sandbox_root.join("sandbox.ready");
        let profile_path = sandbox_root.join("sandbox.sb");

        let perm = if permission_mode.trim().is_empty() {
            "auto"
        } else {
            permission_mode.trim()
        };

        let egress_shim_port =
            crate::sandbox_session::pick_free_loopback_port().map_err(Status::internal)?;
        let loopback_allow_ports = vec![egress_shim_port];

        let runner_argv = vec![
            sandbox_runner_path,
            "--session-id".into(),
            session_id.to_string(),
            "--context-dir".into(),
            context_dir.to_string_lossy().to_string(),
            "--tool-ipc-socket".into(),
            tool_ipc_socket.to_string_lossy().to_string(),
            "--tddy-tools-path".into(),
            tddy_tools_path.clone(),
            "--ready-marker".into(),
            ready_marker.to_string_lossy().to_string(),
            "--claude-binary".into(),
            claude_binary.to_string(),
            "--model".into(),
            model.to_string(),
            "--permission-mode".into(),
            perm.to_string(),
            "--egress-shim-port".into(),
            egress_shim_port.to_string(),
            "--stdio".into(),
        ];

        let mut env = crate::sandbox_session::build_sandbox_runner_env(
            &scratch_home,
            &scratch_tmp,
            session_id,
            &tool_ipc_socket,
            &egress_dir,
        );
        if !specialized_defs.is_empty() {
            env.extend(self.specialized_subagent_env(&specialized_defs)?);
        }

        let mut handle = crate::sandbox_session::spawn_sandbox_runner(
            crate::sandbox_session::SandboxRunnerSpawn {
                project_root: sandbox_root.clone(),
                scratch_dir: scratch_dir.clone(),
                egress_dir: egress_dir.clone(),
                profile_path,
                runner_argv,
                env,
                loopback_allow_ports,
                ipc_socket: Some(tool_ipc_socket.clone()),
                mounts: vec![],
            },
        )
        .map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(&egress_dir);
            let mut status = crate::sandbox_session::sandbox_error_to_status(e);
            status.message = format!("{}\n{logs}", status.message);
            status
        })?;

        crate::sandbox_session::wait_for_sandbox_ready(
            &mut handle,
            &ready_marker,
            std::time::Duration::from_secs(120),
            &egress_dir,
        )
        .await
        .map_err(Status::deadline_exceeded)?;

        let (stdout_tx, _) = tokio::sync::broadcast::channel(256);
        let capture = Arc::new(StdMutex::new(Vec::new()));
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel();

        crate::sandbox_session::dial_and_bridge(
            session_id,
            worktree_path.clone(),
            &mut handle,
            self.task_registry.clone(),
            stdout_tx.clone(),
            Arc::clone(&capture),
            stdin_rx,
        )
        .await
        .map_err(Status::internal)?;

        let pid = handle.pid();
        let state = Arc::new(crate::sandbox_session::SandboxSessionState::new(
            crate::sandbox_session::SandboxSessionStateInit {
                pid,
                worktree_path: worktree_path.clone(),
                stdout_tx,
                capture,
                stdin_tx,
                ready_marker: ready_marker.clone(),
                handle,
            },
        ));
        self.sandbox_manager
            .insert(session_id.to_string(), state)
            .await;

        let now = chrono::Utc::now().to_rfc3339();
        let meta = tddy_core::SessionMetadata {
            session_id: session_id.to_string(),
            project_id: project_id.to_string(),
            created_at: now.clone(),
            updated_at: now,
            status: "active".to_string(),
            repo_path: Some(worktree_path.to_string_lossy().to_string()),
            pid: Some(pid),
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some(model.to_string()),
            activity_status: None,
            hook_token: None,
            sandbox: Some(true),
            agent: None,
            recipe: None,
            specialized_agents: specialized_agents.to_vec(),
        };
        tddy_core::write_session_metadata(&session_dir, &meta)
            .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;

        log::info!(
            target: "tddy_daemon::connection_service",
            "started sandboxed claude-cli session {session_id} pid={pid} worktree={}",
            worktree_path.display()
        );

        Ok(Response::new(StartSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
        }))
    }

    /// Handle `ResumeSession` for `session_type = "claude-cli"` sessions.
    async fn resume_claude_cli_session(
        &self,
        os_user: &str,
        session_id: &str,
        session_dir: PathBuf,
        meta: tddy_core::SessionMetadata,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        if meta.sandbox == Some(true) {
            return self
                .resume_sandboxed_claude_cli_session(os_user, session_id, session_dir, meta)
                .await;
        }
        let model = meta.model.clone().unwrap_or_default();
        let worktree_path = meta
            .repo_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| session_dir.clone());

        let binary_path = self
            .config
            .claude_cli
            .as_ref()
            .map(|c| c.binary_path.as_str())
            .unwrap_or("claude");

        let manager = Arc::clone(&self.claude_cli_manager);
        let session_id_owned = session_id.to_string();
        let binary_owned = binary_path.to_string();

        let handle = manager
            .resume(&session_id_owned, worktree_path, &model, &binary_owned)
            .await
            .map_err(|e| Status::internal(format!("failed to relaunch claude-cli: {}", e)))?;

        let pid = handle.pid;

        // Update .session.yaml with new pid and active status.
        let now = chrono::Utc::now().to_rfc3339();
        let updated = tddy_core::SessionMetadata {
            updated_at: now,
            status: "active".to_string(),
            pid: Some(pid),
            ..meta
        };
        tddy_core::write_session_metadata(&session_dir, &updated)
            .map_err(|e| Status::internal(format!("failed to update session metadata: {}", e)))?;

        log::info!(
            target: "tddy_daemon::connection_service",
            "resumed claude-cli session {} pid={}",
            session_id, pid
        );

        Ok(Response::new(ResumeSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
        }))
    }

    /// Re-spawn and re-dial a sandboxed claude-cli session.
    async fn resume_sandboxed_claude_cli_session(
        &self,
        _os_user: &str,
        session_id: &str,
        session_dir: PathBuf,
        meta: tddy_core::SessionMetadata,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        if let Some(state) = self.sandbox_manager.remove(session_id).await {
            state.stop();
        } else if let Some(pid) = meta.pid {
            crate::sandbox_session::terminate_sandbox_process(pid);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;

        let model = meta.model.clone().unwrap_or_default();
        let worktree_path = meta
            .repo_path
            .as_ref()
            .map(PathBuf::from)
            .ok_or_else(|| Status::internal("sandbox session missing repo_path in metadata"))?;

        let pid = self
            .relaunch_sandboxed_runner(
                session_id,
                &session_dir,
                &worktree_path,
                &model,
                "auto",
                &meta.specialized_agents,
            )
            .await?;

        let now = chrono::Utc::now().to_rfc3339();
        let updated = tddy_core::SessionMetadata {
            updated_at: now,
            status: "active".to_string(),
            pid: Some(pid),
            ..meta
        };
        tddy_core::write_session_metadata(&session_dir, &updated)
            .map_err(|e| Status::internal(format!("failed to update session metadata: {e}")))?;

        Ok(Response::new(ResumeSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
        }))
    }

    /// Spawn sandbox-runner + SessionChannel bridge for an existing session directory.
    #[allow(clippy::too_many_arguments)]
    async fn relaunch_sandboxed_runner(
        &self,
        session_id: &str,
        session_dir: &Path,
        worktree_path: &Path,
        model: &str,
        permission_mode: &str,
        specialized_agents: &[String],
    ) -> Result<u32, Status> {
        let specialized_defs = self.resolve_specialized_agent_defs(specialized_agents)?;
        let sandbox_root = session_dir.join("sandbox");
        let egress_dir = session_dir.join("egress");
        std::fs::create_dir_all(sandbox_root.join(".work").join("home"))
            .map_err(|e| Status::internal(format!("mkdir sandbox scratch: {e}")))?;
        std::fs::create_dir_all(sandbox_root.join(".work").join("tmp"))
            .map_err(|e| Status::internal(format!("mkdir sandbox tmp: {e}")))?;
        std::fs::create_dir_all(sandbox_root.join("context"))
            .map_err(|e| Status::internal(format!("mkdir sandbox context: {e}")))?;
        std::fs::create_dir_all(&egress_dir)
            .map_err(|e| Status::internal(format!("mkdir sandbox egress: {e}")))?;

        let sandbox_root = std::fs::canonicalize(&sandbox_root).unwrap_or(sandbox_root);
        let egress_dir = std::fs::canonicalize(&egress_dir).unwrap_or(egress_dir);
        let scratch_dir = sandbox_root.join(".work");
        let scratch_home = scratch_dir.join("home");
        let scratch_tmp = scratch_dir.join("tmp");
        let context_dir = sandbox_root.join("context");

        let replacement_pairs = subagent_replacement_pairs(&specialized_defs);
        let replacement_refs: Vec<Vec<&str>> = replacement_pairs
            .iter()
            .map(|(_, tools)| tools.iter().map(String::as_str).collect())
            .collect();
        let replacements: Vec<tddy_sandbox::SubagentReplacement<'_>> = replacement_pairs
            .iter()
            .zip(replacement_refs.iter())
            .map(|((name, _), refs)| tddy_sandbox::SubagentReplacement {
                name,
                replaced: refs,
            })
            .collect();
        let ctx =
            crate::sandbox_session::prepare_context_dir_with_subagent(worktree_path, &replacements)
                .map_err(|e| Status::internal(format!("prepare context dir: {e}")))?;
        if context_dir.exists() {
            std::fs::remove_dir_all(&context_dir)
                .map_err(|e| Status::internal(format!("clear context dir: {e}")))?;
        }
        std::fs::create_dir_all(&context_dir)
            .map_err(|e| Status::internal(format!("mkdir context dir: {e}")))?;
        crate::sandbox_session::copy_dir_all(ctx.path(), &context_dir)
            .map_err(|e| Status::internal(format!("copy context dir: {e}")))?;

        let tddy_tools_path = crate::sandbox_session::resolve_tddy_tools_path(
            self.config
                .claude_cli
                .as_ref()
                .and_then(|c| c.tddy_tools_path.as_deref()),
        );
        let claude_binary_cfg = self
            .config
            .claude_cli
            .as_ref()
            .map(|c| c.binary_path.as_str())
            .unwrap_or("claude");
        let canonicalize_exec = |p: &str| -> String {
            if p.contains('/') {
                std::fs::canonicalize(p)
                    .map(|c| c.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| p.to_string())
            } else {
                p.to_string()
            }
        };
        let tddy_tools_path = canonicalize_exec(&tddy_tools_path);
        let sandbox_runner_path =
            canonicalize_exec(&crate::sandbox_session::resolve_sandbox_runner_path());
        let claude_binary = canonicalize_exec(claude_binary_cfg);

        let tool_ipc_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(session_id);
        let ready_marker = sandbox_root.join("sandbox.ready");
        let _ = std::fs::remove_file(&tool_ipc_socket);
        let _ = std::fs::remove_file(&ready_marker);
        let profile_path = sandbox_root.join("sandbox.sb");
        let perm = if permission_mode.trim().is_empty() {
            "auto"
        } else {
            permission_mode.trim()
        };

        let egress_shim_port =
            crate::sandbox_session::pick_free_loopback_port().map_err(Status::internal)?;
        let loopback_allow_ports = vec![egress_shim_port];

        let runner_argv = vec![
            sandbox_runner_path,
            "--session-id".into(),
            session_id.to_string(),
            "--context-dir".into(),
            context_dir.to_string_lossy().to_string(),
            "--tool-ipc-socket".into(),
            tool_ipc_socket.to_string_lossy().to_string(),
            "--tddy-tools-path".into(),
            tddy_tools_path.clone(),
            "--ready-marker".into(),
            ready_marker.to_string_lossy().to_string(),
            "--claude-binary".into(),
            claude_binary,
            "--model".into(),
            model.to_string(),
            "--permission-mode".into(),
            perm.to_string(),
            "--egress-shim-port".into(),
            egress_shim_port.to_string(),
            "--stdio".into(),
        ];

        let mut env = crate::sandbox_session::build_sandbox_runner_env(
            &scratch_home,
            &scratch_tmp,
            session_id,
            &tool_ipc_socket,
            &egress_dir,
        );
        if !specialized_defs.is_empty() {
            env.extend(self.specialized_subagent_env(&specialized_defs)?);
        }

        let mut handle = crate::sandbox_session::spawn_sandbox_runner(
            crate::sandbox_session::SandboxRunnerSpawn {
                project_root: sandbox_root.clone(),
                scratch_dir: scratch_dir.clone(),
                egress_dir: egress_dir.clone(),
                profile_path,
                runner_argv,
                env,
                loopback_allow_ports,
                ipc_socket: Some(tool_ipc_socket.clone()),
                mounts: vec![],
            },
        )
        .map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(&egress_dir);
            let mut status = crate::sandbox_session::sandbox_error_to_status(e);
            status.message = format!("{}\n{logs}", status.message);
            status
        })?;

        crate::sandbox_session::wait_for_sandbox_ready(
            &mut handle,
            &ready_marker,
            std::time::Duration::from_secs(120),
            &egress_dir,
        )
        .await
        .map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(&egress_dir);
            Status::deadline_exceeded(format!("wait for sandbox ready: {e}\n{logs}"))
        })?;

        let (stdout_tx, _) = tokio::sync::broadcast::channel(256);
        let capture = Arc::new(StdMutex::new(Vec::new()));
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel();

        crate::sandbox_session::dial_and_bridge(
            session_id,
            worktree_path.to_path_buf(),
            &mut handle,
            self.task_registry.clone(),
            stdout_tx.clone(),
            Arc::clone(&capture),
            stdin_rx,
        )
        .await
        .map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(&egress_dir);
            Status::internal(format!("dial sandbox SessionChannel: {e}\n{logs}"))
        })?;

        let pid = handle.pid();
        let state = Arc::new(crate::sandbox_session::SandboxSessionState::new(
            crate::sandbox_session::SandboxSessionStateInit {
                pid,
                worktree_path: worktree_path.to_path_buf(),
                stdout_tx,
                capture,
                stdin_tx,
                ready_marker,
                handle,
            },
        ));
        self.sandbox_manager
            .insert(session_id.to_string(), state)
            .await;
        Ok(pid)
    }
}

/// Build the (name, replaced-tools) pairs for every resolved specialized-agent def — each its own
/// name + its own YAML-declared `replaces`, normalized.
fn subagent_replacement_pairs(
    specialized_defs: &[tddy_discovery::agent_def::SpecializedAgentDef],
) -> Vec<(String, Vec<String>)> {
    specialized_defs
        .iter()
        .map(|def| {
            (
                def.name.clone(),
                tddy_discovery::subagent::normalize_replaced_tools(&def.replaces),
            )
        })
        .collect()
}

/// Merge local `ListProjects` rows with [`EligibleDaemonSource::peer_project_entries`].
fn merge_listed_projects_with_peers(
    eligible: &dyn EligibleDaemonSource,
    session_token: &str,
    local: Vec<ProtoProjectEntry>,
) -> Vec<ProtoProjectEntry> {
    let peer_rows = eligible.peer_project_entries(session_token);
    log::debug!(
        target: "tddy_daemon::connection_service",
        "merge_listed_projects_with_peers: local_rows={} peer_rows={} (session_token len={})",
        local.len(),
        peer_rows.len(),
        session_token.len()
    );
    let mut merged = local;
    let n_append = peer_rows.len();
    merged.extend(peer_rows);
    log::info!(
        target: "tddy_daemon::connection_service",
        "merge_listed_projects_with_peers: merged_total={} appended_from_peers={}",
        merged.len(),
        n_append
    );
    merged
}

#[async_trait::async_trait]
impl ConnectionServiceTrait for ConnectionServiceImpl {
    async fn list_tools(
        &self,
        _request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        self.record_rpc_activity();
        let tools: Vec<ToolInfo> = self
            .config
            .allowed_tools()
            .iter()
            .map(|t| {
                let label = t
                    .label
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| t.path.clone());
                ToolInfo {
                    path: t.path.clone(),
                    label,
                }
            })
            .collect();
        Ok(Response::new(ListToolsResponse { tools }))
    }

    async fn list_agents(
        &self,
        _request: Request<ListAgentsRequest>,
    ) -> Result<Response<ListAgentsResponse>, Status> {
        log::debug!("list_agents RPC: mapping config allowlist to AgentInfo");
        let agents: Vec<AgentInfo> = agent_allowlist_rows(&self.config)
            .into_iter()
            .map(|row| AgentInfo {
                id: row.id,
                label: row.display_label,
            })
            .collect();
        log::info!("list_agents RPC: returning {} agent(s)", agents.len());
        Ok(Response::new(ListAgentsResponse { agents }))
    }

    /// Resolved specialized-agent defs (builtin + `<tddyhome>/agents/*.yaml` — see
    /// docs/ft/coder/specialized-subagents.md) available to wire into a managed-codebase session.
    async fn list_subagents(
        &self,
        _request: Request<ListSubagentsRequest>,
    ) -> Result<Response<ListSubagentsResponse>, Status> {
        log::debug!("list_subagents RPC: resolving <tddyhome>/agents defs");
        let agents_dir = self.tddy_data_dir.join("agents");
        let subagents: Vec<SubagentInfo> =
            tddy_discovery::agent_def::resolve_agent_defs(&agents_dir)
                .into_iter()
                .map(|def| {
                    let label = def
                        .label
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| def.name.clone());
                    SubagentInfo {
                        name: def.name,
                        label,
                        model: def.model,
                    }
                })
                .collect();
        log::info!(
            "list_subagents RPC: returning {} subagent(s)",
            subagents.len()
        );
        Ok(Response::new(ListSubagentsResponse { subagents }))
    }

    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let timeout = self.config.spawn_worker_request_timeout();
        let sessions_base_blocking = sessions_base.clone();
        let local_daemon_id = local_instance_id_for_config(&self.config);
        let entries =
            spawn_blocking_with_timeout(timeout, "ListSessions: read and enrich", move || {
                let sessions = session_reader::list_sessions_in_dir(&sessions_base_blocking)
                    .map_err(|e| anyhow::anyhow!(e))?;
                let mut out = Vec::with_capacity(sessions.len());
                for s in sessions {
                    let session_dir = sessions_base_blocking
                        .join(SESSIONS_SUBDIR)
                        .join(&s.session_id);
                    let mut entry = ProtoSessionEntry {
                        session_id: s.session_id,
                        created_at: s.created_at,
                        status: s.status,
                        repo_path: s.repo_path,
                        pid: s.pid.unwrap_or(0),
                        is_active: s.is_active,
                        project_id: s.project_id,
                        daemon_instance_id: local_daemon_id.clone(),
                        workflow_goal: String::new(),
                        workflow_state: String::new(),
                        elapsed_display: String::new(),
                        agent: String::new(),
                        model: String::new(),
                        pending_elicitation: false,
                        activity_status: String::new(),
                        tool: s.tool,
                        session_type: s.session_type,
                        updated_at: s.updated_at,
                        livekit_room: s.livekit_room,
                        previous_session_id: s.previous_session_id,
                        orchestrator_session_id: String::new(),
                        recipe: String::new(),
                        stack_plan_json: String::new(),
                    };
                    if let Err(e) = session_list_enrichment::apply_session_list_status_to_proto(
                        &session_dir,
                        &mut entry,
                    ) {
                        log::warn!(
                            target: "tddy_daemon::connection_service",
                            "ListSessions: enrichment failed for {}: {}",
                            session_dir.display(),
                            e
                        );
                    }
                    out.push(entry);
                }
                Ok(out)
            })
            .await?;
        Ok(Response::new(ListSessionsResponse { sessions: entries }))
    }

    async fn list_projects(
        &self,
        request: Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let projects = project_storage::read_projects(&projects_dir)
            .map_err(|e| Status::internal(e.to_string()))?;
        let local_daemon_id = local_instance_id_for_config(&self.config);
        let entries: Vec<ProtoProjectEntry> = projects
            .into_iter()
            .map(|p| ProtoProjectEntry {
                project_id: p.project_id,
                name: p.name,
                git_url: p.git_url,
                main_repo_path: p.main_repo_path,
                daemon_instance_id: local_daemon_id.clone(),
            })
            .collect();
        log::debug!(
            target: "tddy_daemon::connection_service",
            "list_projects: local_registry_rows={} local_daemon_instance_id={}",
            entries.len(),
            local_daemon_id
        );
        let merged = merge_listed_projects_with_peers(
            &*self.eligible_daemon_source,
            &req.session_token,
            entries,
        );
        Ok(Response::new(ListProjectsResponse { projects: merged }))
    }

    async fn create_project(
        &self,
        request: Request<CreateProjectRequest>,
    ) -> Result<Response<CreateProjectResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let name = req.name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("project name is required"));
        }
        if name.contains('/') || name.contains("..") {
            return Err(Status::invalid_argument("invalid project name"));
        }
        let git_url = req.git_url.trim();
        if git_url.is_empty() {
            return Err(Status::invalid_argument("git_url is required"));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;

        let user_rel = req.user_relative_path.trim();
        let destination = if !user_rel.is_empty() {
            project_path_under_home_from_user_relative(os_user, user_rel)
                .map_err(Status::invalid_argument)?
        } else {
            let base = repos_base_for_user(os_user, self.config.repos_base_path_or_default())
                .ok_or_else(|| Status::internal("could not resolve repos base path"))?;
            base.join(name)
        };
        let spawn_client = self.spawn_client.clone();
        let os_user_owned = os_user.to_string();
        let git_url_owned = git_url.to_string();
        let dest_path = destination.clone();
        let timeout = self.config.spawn_worker_request_timeout();

        spawn_blocking_with_timeout(timeout, "create_project: clone_repo", move || {
            if let Some(ref client) = spawn_client {
                client.clone_repo(spawn_worker::CloneRequest {
                    os_user: os_user_owned,
                    git_url: git_url_owned,
                    destination: dest_path.display().to_string(),
                })
            } else {
                spawner::clone_as_user(&os_user_owned, &git_url_owned, &dest_path)
            }
        })
        .await?;

        let main_repo_path = destination
            .canonicalize()
            .unwrap_or(destination)
            .display()
            .to_string();

        let project = ProjectData {
            project_id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            git_url: git_url.to_string(),
            main_repo_path,
            main_branch_ref: None,
            host_repo_paths: std::collections::HashMap::new(),
        };
        let entry = ProtoProjectEntry {
            project_id: project.project_id.clone(),
            name: project.name.clone(),
            git_url: project.git_url.clone(),
            main_repo_path: project.main_repo_path.clone(),
            daemon_instance_id: local_instance_id_for_config(&self.config),
        };
        project_storage::add_project(&projects_dir, project)
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateProjectResponse {
            project: Some(entry),
        }))
    }

    async fn start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let agent_trim = req.agent.trim();
        if !agent_trim.is_empty() {
            let allowed = self.config.allowed_agents();
            if !allowed.is_empty() && !allowed.iter().any(|a| a.id == agent_trim) {
                return Err(Status::invalid_argument(format!(
                    "agent id {:?} is not listed in allowed_agents (configure daemon YAML)",
                    agent_trim
                )));
            }
        }

        let requested_daemon = req.daemon_instance_id.trim();
        let local_id = local_instance_id_for_config(&self.config);
        let eligible_rows = self.eligible_daemon_source.list_eligible_daemons();
        let eligible_ids: Vec<String> = eligible_rows
            .iter()
            .map(|e| e.instance_id.0.clone())
            .collect();
        let route = match crate::livekit_peer_discovery::classify_start_session_peer_route(
            &local_id,
            requested_daemon,
            &eligible_ids,
        ) {
            Ok(r) => r,
            Err(msg) => {
                log::info!("StartSession: rejected daemon routing: {}", msg);
                return Err(Status::failed_precondition(msg));
            }
        };

        match route {
            crate::livekit_peer_discovery::StartSessionPeerRoute::Forward { peer_instance_id } => {
                log::info!(
                    "StartSession: forwarding RPC to remote daemon_instance_id={}",
                    peer_instance_id
                );
                let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                    Status::failed_precondition(
                        "cannot forward StartSession: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)",
                    )
                })?;
                let inner = crate::livekit_peer_discovery::forward_start_session_via_livekit(
                    slot,
                    &peer_instance_id,
                    &req,
                )
                .await?;
                log::info!(
                    "StartSession: forward succeeded session_id={} livekit_server_identity={}",
                    inner.session_id,
                    inner.livekit_server_identity
                );
                return Ok(Response::new(inner));
            }
            crate::livekit_peer_discovery::StartSessionPeerRoute::Local => {}
        }

        // --- workspace branch: no LiveKit, no PTY; resolves project, creates a git worktree ---
        if req.session_type.trim() == "workspace" {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            let session_id = Uuid::now_v7().to_string();
            let timeout = self.config.spawn_worker_request_timeout();
            return workspace_session::start_workspace_session(
                os_user,
                &session_id,
                sessions_base,
                req.project_id.trim(),
                &self.tddy_data_dir,
                timeout,
            )
            .await;
        }

        // --- claude-cli branch: no LiveKit; resolves project and creates a real git worktree ---
        if req.session_type.trim() == "claude-cli" {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            let session_id = Uuid::now_v7().to_string();
            let stack_parent_for_claude_cli: Option<String> = {
                let t = req.stack_parent.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            };
            if req.sandbox {
                return self
                    .start_sandboxed_claude_cli_session(
                        os_user,
                        &session_id,
                        sessions_base,
                        req.model.trim(),
                        req.project_id.trim(),
                        req.branch_worktree_intent.trim(),
                        req.new_branch_name.trim(),
                        req.selected_integration_base_ref.trim(),
                        req.selected_branch_to_work_on.trim(),
                        req.permission_mode.trim(),
                        stack_parent_for_claude_cli.as_deref(),
                        req.managed_codebase,
                        &req.specialized_agents,
                    )
                    .await;
            }
            return self
                .start_claude_cli_session(
                    os_user,
                    &session_id,
                    sessions_base,
                    req.model.trim(),
                    req.project_id.trim(),
                    req.branch_worktree_intent.trim(),
                    req.new_branch_name.trim(),
                    req.selected_integration_base_ref.trim(),
                    req.selected_branch_to_work_on.trim(),
                    req.initial_prompt.trim(),
                    req.permission_mode.trim(),
                    stack_parent_for_claude_cli.as_deref(),
                )
                .await;
        }

        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| Status::failed_precondition("LiveKit not configured"))?;

        let project_id_req = req.project_id.trim();
        if project_id_req.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id_req)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;

        let repo_path = Path::new(&project.main_repo_path);
        if !repo_path.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        log::debug!("StartSession: entering spawn_blocking session_id=new");
        let spawn_client = self.spawn_client.clone();
        let spawn_mouse = self.config.spawn_mouse;
        let os_user = os_user.to_string();
        let tool_path = req.tool_path.clone();
        let tddy_data_dir_for_spawn = self.tddy_data_dir.clone();
        let repo_path = repo_path.to_path_buf();
        let livekit = livekit.clone();
        let pid_for_spawn = project.project_id.clone();
        let agent_for_spawn: Option<String> = {
            let t = req.agent.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let recipe_for_spawn: Option<String> = {
            let t = req.recipe.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let stack_parent_for_spawn: Option<String> = {
            let t = req.stack_parent.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let timeout = self.config.spawn_worker_request_timeout();
        let daemon_log = self.config.log.clone();
        let result = spawn_blocking_with_timeout(timeout, "StartSession: spawn", move || {
            log::debug!(
                "StartSession: spawn_blocking running, using_spawn_worker={}",
                spawn_client.is_some()
            );
            let pid = Some(pid_for_spawn.as_str());
            let agent = agent_for_spawn.as_deref();
            let recipe = recipe_for_spawn.as_deref();
            let stack_parent = stack_parent_for_spawn.as_deref();
            if let Some(ref client) = spawn_client {
                let spawn_req = spawn_worker::build_spawn_request(
                    &os_user,
                    &tool_path,
                    &tddy_data_dir_for_spawn,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: None,
                        new_session_id: None,
                        project_id: pid,
                        agent,
                        mouse: spawn_mouse,
                        recipe,
                        stack_parent,
                    },
                    daemon_log.as_ref(),
                );
                client.spawn(spawn_req)
            } else {
                let (child_log_level, child_log_format) =
                    spawner::child_log_yaml_tuning(daemon_log.as_ref());
                spawner::spawn_as_user(
                    &os_user,
                    &tool_path,
                    &tddy_data_dir_for_spawn,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: None,
                        new_session_id: None,
                        project_id: pid,
                        agent,
                        mouse: spawn_mouse,
                        recipe,
                        stack_parent,
                    },
                    child_log_level.as_str(),
                    child_log_format.as_str(),
                )
            }
        })
        .await?;
        log::debug!(
            "StartSession: spawn_blocking returned, session_id={}",
            result.session_id
        );
        self.maybe_spawn_telegram_observer(&result.session_id, result.grpc_port);
        Ok(Response::new(StartSessionResponse {
            session_id: result.session_id,
            livekit_room: result.livekit_room,
            livekit_url: result.livekit_url,
            livekit_server_identity: result.livekit_server_identity,
        }))
    }

    async fn connect_session(
        &self,
        request: Request<ConnectSessionRequest>,
    ) -> Result<Response<ConnectSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        // claude-cli and workspace sessions do not use LiveKit — return empty fields immediately.
        if metadata.session_type.as_deref() == Some("claude-cli")
            || metadata.session_type.as_deref() == Some("workspace")
        {
            return Ok(Response::new(ConnectSessionResponse {
                livekit_room: String::new(),
                livekit_url: String::new(),
                livekit_server_identity: String::new(),
            }));
        }

        let livekit_url = self
            .config
            .livekit
            .as_ref()
            .and_then(|l| l.public_url.clone())
            .or_else(|| self.config.livekit.as_ref().and_then(|l| l.url.clone()))
            .ok_or_else(|| Status::internal("LiveKit URL not configured"))?;
        let livekit_room = metadata
            .livekit_room
            .ok_or_else(|| Status::failed_precondition("session has no LiveKit room"))?;
        let instance = spawner::livekit_spawn_daemon_instance_id(&self.config);
        let livekit_server_identity =
            spawner::livekit_server_identity_for_session(instance.as_deref(), &req.session_id);
        log::debug!(
            "ConnectSession: livekit_server_identity={} session_id={}",
            livekit_server_identity,
            req.session_id
        );
        Ok(Response::new(ConnectSessionResponse {
            livekit_room,
            livekit_url,
            livekit_server_identity,
        }))
    }

    async fn resume_session(
        &self,
        request: Request<ResumeSessionRequest>,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        // --- claude-cli branch: resume without LiveKit ---
        if metadata.session_type.as_deref() == Some("claude-cli") {
            return self
                .resume_claude_cli_session(os_user, &req.session_id, session_dir, metadata)
                .await;
        }

        let repo_path = metadata
            .repo_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| session_dir.clone());
        let repo_path = if repo_path.exists() {
            repo_path
        } else {
            session_dir.clone()
        };
        let tool_path = metadata
            .tool
            .clone()
            .ok_or_else(|| Status::failed_precondition("session has no recorded tool path"))?;
        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| Status::failed_precondition("LiveKit not configured"))?;
        let spawn_client = self.spawn_client.clone();
        let spawn_mouse = self.config.spawn_mouse;
        let os_user = os_user.to_string();
        let session_id = req.session_id.clone();
        let livekit = livekit.clone();
        let project_id_resume = metadata.project_id.clone();
        let (resume_agent, resume_recipe) = resume_agent_and_recipe(&metadata);
        let tddy_data_dir_for_spawn = self.tddy_data_dir.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let daemon_log = self.config.log.clone();
        let result = spawn_blocking_with_timeout(timeout, "ResumeSession: spawn", move || {
            let pid = if project_id_resume.is_empty() {
                None
            } else {
                Some(project_id_resume.as_str())
            };
            if let Some(ref client) = spawn_client {
                let spawn_req = spawn_worker::build_spawn_request(
                    &os_user,
                    &tool_path,
                    &tddy_data_dir_for_spawn,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: Some(session_id.as_str()),
                        new_session_id: None,
                        project_id: pid,
                        agent: resume_agent.as_deref(),
                        mouse: spawn_mouse,
                        recipe: resume_recipe.as_deref(),
                        stack_parent: None,
                    },
                    daemon_log.as_ref(),
                );
                client.spawn(spawn_req)
            } else {
                let (child_log_level, child_log_format) =
                    spawner::child_log_yaml_tuning(daemon_log.as_ref());
                spawner::spawn_as_user(
                    &os_user,
                    &tool_path,
                    &tddy_data_dir_for_spawn,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: Some(session_id.as_str()),
                        new_session_id: None,
                        project_id: pid,
                        agent: resume_agent.as_deref(),
                        mouse: spawn_mouse,
                        recipe: resume_recipe.as_deref(),
                        stack_parent: None,
                    },
                    child_log_level.as_str(),
                    child_log_format.as_str(),
                )
            }
        })
        .await?;
        self.maybe_spawn_telegram_observer(&result.session_id, result.grpc_port);
        Ok(Response::new(ResumeSessionResponse {
            session_id: result.session_id,
            livekit_room: result.livekit_room,
            livekit_url: result.livekit_url,
            livekit_server_identity: result.livekit_server_identity,
        }))
    }

    async fn signal_session(
        &self,
        request: Request<SignalSessionRequest>,
    ) -> Result<Response<SignalSessionResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "SignalSession: session_id={}, signal={}",
            req.session_id,
            req.signal
        );

        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        let pid = metadata
            .pid
            .ok_or_else(|| Status::failed_precondition("session has no PID"))?;

        log::debug!(
            "SignalSession: resolved pid={} for session={}",
            pid,
            req.session_id
        );

        #[cfg(unix)]
        {
            let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
            if !alive {
                log::debug!("SignalSession: pid={} is not alive", pid);
                return Err(Status::failed_precondition("process is not alive"));
            }

            let os_signal = match Signal::try_from(req.signal) {
                Ok(Signal::Sigint) => libc::SIGINT,
                Ok(Signal::Sigterm) => libc::SIGTERM,
                Ok(Signal::Sigkill) => libc::SIGKILL,
                Err(_) => return Err(Status::invalid_argument("invalid signal value")),
            };

            log::info!(
                "SignalSession: sending signal {} to pid={} session={}",
                os_signal,
                pid,
                req.session_id
            );

            let ret = unsafe { libc::kill(pid as i32, os_signal) };
            if ret != 0 {
                let err = std::io::Error::last_os_error();
                log::error!(
                    "SignalSession: kill({}, {}) failed: {}",
                    pid,
                    os_signal,
                    err
                );
                return Err(Status::internal(format!("failed to send signal: {}", err)));
            }

            Ok(Response::new(SignalSessionResponse {
                ok: true,
                message: format!("signal {} sent to pid {}", os_signal, pid),
            }))
        }

        #[cfg(not(unix))]
        {
            let _ = pid;
            Err(Status::unimplemented(
                "signal delivery is only supported on Unix",
            ))
        }
    }

    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }
        log::debug!("DeleteSession: requested session_id={}", session_id);
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        log::debug!(
            "DeleteSession: resolved sessions_base={:?} for os_user={}",
            sessions_base,
            os_user
        );
        let projects_dir_opt = projects_path_for_user(os_user, Some(&self.tddy_data_dir));
        if let Some(sandbox) = self.sandbox_manager.get(session_id).await {
            sandbox.stop();
        }
        let _ = self.sandbox_manager.remove(session_id).await;
        session_deletion::delete_session_directory(
            &sessions_base,
            session_id,
            projects_dir_opt.as_deref(),
        )?;
        log::info!("DeleteSession: successfully removed session {}", session_id);
        Ok(Response::new(DeleteSessionResponse { ok: true }))
    }

    async fn list_eligible_daemons(
        &self,
        request: Request<ListEligibleDaemonsRequest>,
    ) -> Result<Response<ListEligibleDaemonsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let local_id = local_instance_id_for_config(&self.config);
        let daemons: Vec<EligibleDaemonEntry> = self
            .eligible_daemon_source
            .list_eligible_daemons()
            .into_iter()
            .map(|entry| EligibleDaemonEntry {
                instance_id: entry.instance_id.0.clone(),
                label: entry.label,
                is_local: entry.instance_id.0 == local_id,
            })
            .collect();

        Ok(Response::new(ListEligibleDaemonsResponse { daemons }))
    }

    async fn list_session_workflow_files(
        &self,
        request: Request<ListSessionWorkflowFilesRequest>,
    ) -> Result<Response<ListSessionWorkflowFilesResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "ListSessionWorkflowFiles: session_id={}",
            req.session_id.trim()
        );
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        log::debug!(
            "ListSessionWorkflowFiles: resolved session_dir={:?}",
            session_dir
        );
        let basenames =
            crate::session_workflow_files::list_allowlisted_workflow_basenames(&session_dir)?;
        let n = basenames.len();
        let files: Vec<WorkflowFileEntry> = basenames
            .into_iter()
            .map(|basename| WorkflowFileEntry { basename })
            .collect();
        log::info!(
            "ListSessionWorkflowFiles: returning {} file(s) for session_id={}",
            n,
            req.session_id.trim()
        );
        Ok(Response::new(ListSessionWorkflowFilesResponse { files }))
    }

    async fn read_session_workflow_file(
        &self,
        request: Request<ReadSessionWorkflowFileRequest>,
    ) -> Result<Response<ReadSessionWorkflowFileResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "ReadSessionWorkflowFile: session_id={} basename={:?}",
            req.session_id.trim(),
            req.basename
        );
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let content_utf8 = crate::session_workflow_files::read_allowlisted_workflow_file_utf8(
            &session_dir,
            &req.basename,
        )?;
        log::info!(
            "ReadSessionWorkflowFile: success session_id={} basename={:?} bytes={}",
            req.session_id.trim(),
            req.basename,
            content_utf8.len()
        );
        Ok(Response::new(ReadSessionWorkflowFileResponse {
            content_utf8,
        }))
    }

    async fn list_worktrees_for_project(
        &self,
        request: Request<ListWorktreesForProjectRequest>,
    ) -> Result<Response<ListWorktreesForProjectResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let project_id = req.project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;

        let local_id = local_instance_id_for_config(&self.config);
        let main_repo_str =
            project_storage::main_repo_path_for_host(&projects_dir, project_id, local_id.as_str())
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("project not found"))?;

        let main_repo = PathBuf::from(&main_repo_str);
        if !main_repo.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        let cache = Arc::clone(&self.worktree_stats_cache);
        let pid = project_id.to_string();
        let repo = main_repo.clone();
        let refresh = req.refresh;
        let timeout = self.config.spawn_worker_request_timeout();

        let snapshots = spawn_blocking_with_timeout(
            timeout,
            "ListWorktreesForProject: cache read/refresh",
            move || {
                if refresh {
                    cache.refresh_stats_for_project(&pid, &repo);
                }
                Ok(cache.list_cached_stats(&pid))
            },
        )
        .await?;

        let worktrees: Vec<WorktreeRow> = snapshots
            .into_iter()
            .map(|s| WorktreeRow {
                path: s.path.to_string_lossy().to_string(),
                branch_label: s.branch_label,
                disk_bytes: s.disk_bytes,
                changed_files: s.changed_files,
                lines_added: s.lines_added,
                lines_removed: s.lines_removed,
                updated_at_unix_ms: s.updated_at_unix_ms,
                stale: s.stale,
            })
            .collect();

        Ok(Response::new(ListWorktreesForProjectResponse { worktrees }))
    }

    /// Associated output stream type for [`stream_session_terminal_io`].
    type StreamSessionTerminalIoStream = TerminalOutputStream;

    async fn stream_session_terminal_io(
        &self,
        request: Request<Streaming<SessionTerminalInput>>,
    ) -> Result<Response<Self::StreamSessionTerminalIoStream>, Status> {
        let mut in_stream = request.into_inner();

        // Read the first message to get session_id and session_token for auth.
        let first: SessionTerminalInput = in_stream
            .next()
            .await
            .ok_or_else(|| Status::invalid_argument("stream ended before first message"))?
            .map_err(|e| Status::internal(e.to_string()))?;

        let github_user = (self.user_resolver)(&first.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = first.session_id.clone();
        let terminal_id = resolved_terminal_id(&first.terminal_id).to_string();
        log::info!(
            target: "tddy_daemon::connection_service",
            "stream_session_terminal_io: session_id={} terminal_id={}",
            session_id,
            terminal_id
        );

        if let Some(sandbox) = self.sandbox_manager.get(&session_id).await {
            if terminal_id != MAIN_TERMINAL_ID {
                return Err(Status::not_found("terminal not found or not running"));
            }
            let stdin_tx = sandbox.stdin_tx.clone();
            let stdout_rx = sandbox.stdout_tx.subscribe();
            if !first.data.is_empty() {
                let _ = stdin_tx.send(bytes::Bytes::from(first.data));
            }
            let stdin_tx2 = stdin_tx.clone();
            tokio::spawn(async move {
                while let Some(Ok(msg)) = in_stream.next().await {
                    if !msg.data.is_empty() {
                        let _ = stdin_tx2.send(bytes::Bytes::from(msg.data));
                    }
                }
            });
            return Ok(Response::new(TerminalOutputStream { rx: stdout_rx }));
        }

        if !self
            .claude_cli_manager
            .verify_control(&session_id, &first.control_token)
            .await
        {
            return Err(Status::failed_precondition(
                "terminal controlled by another screen",
            ));
        }

        let handle = self
            .claude_cli_manager
            .get_terminal(&session_id, &terminal_id)
            .await
            .ok_or_else(|| {
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "stream_session_terminal_io: session {} terminal {} not found in registry",
                    session_id,
                    terminal_id
                );
                Status::not_found("terminal not found or not running")
            })?;

        let stdin_tx = handle.stdin_tx.clone();
        let stdout_rx = handle.stdout_tx.subscribe();

        // Trigger a SIGWINCH so claude redraws its full screen onto the now-subscribed channel.
        // The initial render happens before the browser's first stream call arrives (network RTT),
        // so without this the terminal would be blank until the user sends input.
        handle.trigger_redraw();

        // Forward the first data chunk (if any).
        if !first.data.is_empty() {
            let _ = stdin_tx.send(bytes::Bytes::from(first.data));
        }

        // Spawn a task to forward subsequent input chunks to stdin.
        let stdin_tx2 = stdin_tx.clone();
        let manager2 = Arc::clone(&self.claude_cli_manager);
        tokio::spawn(async move {
            while let Some(Ok(msg)) = in_stream.next().await {
                if !manager2
                    .verify_control(&session_id, &msg.control_token)
                    .await
                {
                    break;
                }
                if !msg.data.is_empty() {
                    let _ = stdin_tx2.send(bytes::Bytes::from(msg.data));
                }
            }
        });

        Ok(Response::new(TerminalOutputStream { rx: stdout_rx }))
    }

    /// Associated output stream type for [`stream_terminal_output`].
    type StreamTerminalOutputStream = MpscTerminalOutputStream;

    /// Server-streaming output — browser-compatible alternative to the bidi `StreamSessionTerminalIO`.
    /// connect-web's Fetch transport cannot send streaming request bodies, so bidi streaming never
    /// reaches the daemon from a browser. This RPC provides the output half; input goes via the
    /// unary `SendTerminalInput`.
    async fn stream_terminal_output(
        &self,
        request: Request<StreamTerminalOutputRequest>,
    ) -> Result<Response<Self::StreamTerminalOutputStream>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        let terminal_id = resolved_terminal_id(&req.terminal_id).to_string();
        log::info!(
            target: "tddy_daemon::connection_service",
            "stream_terminal_output: session_id={} terminal_id={}",
            session_id,
            terminal_id
        );

        if let Some(sandbox) = self.sandbox_manager.get(&session_id).await {
            if terminal_id != MAIN_TERMINAL_ID {
                return Err(Status::not_found("terminal not found or not running"));
            }
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let historical = sandbox
                .capture
                .lock()
                .map(|cap| bytes::Bytes::copy_from_slice(&cap))
                .unwrap_or_default();
            if !historical.is_empty() {
                let _ = tx.send(historical);
            }
            let mut stdout_rx = sandbox.stdout_tx.subscribe();
            tokio::spawn(async move {
                loop {
                    match stdout_rx.recv().await {
                        Ok(chunk) => {
                            if tx.send(chunk).is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            return Ok(Response::new(MpscTerminalOutputStream { rx }));
        }

        let handle = self
            .claude_cli_manager
            .get_terminal(&session_id, &terminal_id)
            .await
            .ok_or_else(|| {
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "stream_terminal_output: session {} terminal {} not found in registry",
                    session_id,
                    terminal_id
                );
                Status::not_found("terminal not found or not running")
            })?;

        // If the client supplied terminal dimensions, resize the PTY before replay so that
        // the TUI redraws at the browser's actual width rather than the PTY's spawn-time default.
        let has_initial_dims = req.initial_cols > 0 && req.initial_rows > 0;
        if has_initial_dims {
            handle.resize(req.initial_rows as u16, req.initial_cols as u16);
            log::debug!(
                target: "tddy_daemon::connection_service",
                "stream_terminal_output: resized PTY to {}×{} for session {} before replay",
                req.initial_cols,
                req.initial_rows,
                session_id
            );
        } else {
            log::debug!(
                target: "tddy_daemon::connection_service",
                "stream_terminal_output: no initial dimensions from client for session {} (cols={} rows={}) — replay will use PTY default size",
                session_id, req.initial_cols, req.initial_rows
            );
        }

        // Subscribe to broadcast BEFORE reading the capture buffer so there is no gap:
        // bytes produced between the capture snapshot and the first bridge recv() are
        // covered by the broadcast subscription.
        let (mpsc_tx, mpsc_rx) = tokio::sync::mpsc::unbounded_channel::<bytes::Bytes>();
        let mut broadcast_rx = handle.stdout_tx.subscribe();

        // Replay capture buffer only when the client did NOT supply terminal dimensions.
        //
        // When dimensions are provided we already sent SIGWINCH (above), which will make the
        // TUI redraw at the correct size. Replaying the capture buffer here would send
        // pre-resize content (drawn at the PTY's old width) to the browser before the
        // post-SIGWINCH redraw arrives via broadcast — producing garbled output. Skipping
        // the replay means the client sees a clean fresh frame once the TUI redraws.
        //
        // When no dimensions are provided (legacy / fallback path) the replay is still the
        // only way to see historical content, so we keep it.
        let replay_capture = !has_initial_dims;
        if replay_capture {
            let historical = handle
                .capture
                .lock()
                .map(|cap| bytes::Bytes::copy_from_slice(&cap))
                .unwrap_or_default();
            if !historical.is_empty() {
                log::debug!(
                    target: "tddy_daemon::connection_service",
                    "stream_terminal_output: replaying {} capture bytes for session {}",
                    historical.len(),
                    session_id
                );
                let _ = mpsc_tx.send(historical);
            }
        } else {
            log::debug!(
                target: "tddy_daemon::connection_service",
                "stream_terminal_output: skipping capture replay for session {} (dimensions {}×{} provided; relying on SIGWINCH redraw via broadcast)",
                session_id, req.initial_cols, req.initial_rows
            );
        }

        // Trigger a redraw before draining: this queues a second SIGWINCH so the TUI will
        // produce a fresh frame at the correct size. The drain below discards any pre-resize
        // output already buffered in the broadcast receiver, so the bridge task (started after
        // the drain) only forwards the post-SIGWINCH fresh frame to the browser.
        handle.trigger_redraw();

        // When the client supplied terminal dimensions we already resized the PTY (above).
        // Drain any messages that arrived in the broadcast receiver between subscribe() and
        // now — those were produced at the old PTY width (220 cols) and would cause garbled
        // output if forwarded. The trigger_redraw() above guarantees a fresh post-resize
        // frame will be produced, so discarding the stale buffer is safe.
        if has_initial_dims {
            use tokio::sync::broadcast::error::TryRecvError;
            let mut drained = 0usize;
            loop {
                match broadcast_rx.try_recv() {
                    Ok(_) => drained += 1,
                    Err(TryRecvError::Lagged(_)) => continue,
                    Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
                }
            }
            if drained > 0 {
                log::debug!(
                    target: "tddy_daemon::connection_service",
                    "stream_terminal_output: drained {} stale pre-resize broadcast message(s) for session {}",
                    drained, session_id
                );
            }
        }

        // Bridge broadcast → mpsc for all future output.
        // Also breaks when pty_done fires so the HTTP stream ends when the process exits.
        let mpsc_tx_bridge = mpsc_tx.clone();
        let mut pty_done = handle.pty_done.clone();
        tokio::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                tokio::select! {
                    result = broadcast_rx.recv() => {
                        match result {
                            Ok(chunk) => {
                                if mpsc_tx_bridge.send(chunk).is_err() {
                                    break; // receiver dropped (stream closed)
                                }
                            }
                            Err(RecvError::Closed) => break,
                            Err(RecvError::Lagged(_)) => continue, // skip lagged; resume from latest
                        }
                    }
                    _ = pty_done.changed() => break,
                }
            }
        });

        Ok(Response::new(MpscTerminalOutputStream { rx: mpsc_rx }))
    }

    /// Unary input — browser-compatible alternative to the client-streaming half of `StreamSessionTerminalIO`.
    async fn send_terminal_input(
        &self,
        request: Request<SessionTerminalInput>,
    ) -> Result<Response<SendTerminalInputResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        let terminal_id = resolved_terminal_id(&req.terminal_id).to_string();

        if let Some(sandbox) = self.sandbox_manager.get(&session_id).await {
            if terminal_id != MAIN_TERMINAL_ID {
                return Err(Status::not_found("terminal not found or not running"));
            }
            if !req.data.is_empty() {
                let _ = sandbox.stdin_tx.send(bytes::Bytes::from(req.data));
            }
            return Ok(Response::new(SendTerminalInputResponse {}));
        }

        if !self
            .claude_cli_manager
            .verify_control(&session_id, &req.control_token)
            .await
        {
            return Err(Status::failed_precondition(
                "terminal controlled by another screen",
            ));
        }

        let handle = self
            .claude_cli_manager
            .get_terminal(&session_id, &terminal_id)
            .await
            .ok_or_else(|| Status::not_found("terminal not found or not running"))?;

        if !req.data.is_empty() {
            log::trace!(
                target: "tddy_daemon::connection_service",
                "send_terminal_input: session_id={} terminal_id={} {} bytes: {:?}",
                session_id,
                terminal_id,
                req.data.len(),
                String::from_utf8_lossy(&req.data)
            );
            handle.send_input(bytes::Bytes::from(req.data));
        }
        Ok(Response::new(SendTerminalInputResponse {}))
    }

    async fn start_terminal_session(
        &self,
        request: Request<StartTerminalSessionRequest>,
    ) -> Result<Response<StartTerminalSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        // A Bash tool runs in the session's worktree, resolved from the main (claude) terminal.
        let main = self
            .claude_cli_manager
            .get(&session_id)
            .await
            .ok_or_else(|| Status::failed_precondition("session has no running terminal"))?;
        let worktree = main.worktree_path.clone();

        // The Bash tool is built-in: the user's login shell, falling back to /bin/bash.
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

        let handle = self
            .claude_cli_manager
            .start_terminal(&session_id, worktree, &shell)
            .await
            .map_err(|e| Status::internal(format!("failed to start terminal: {e}")))?;

        Ok(Response::new(StartTerminalSessionResponse {
            terminal_id: handle.terminal_id.clone(),
        }))
    }

    async fn stop_terminal_session(
        &self,
        request: Request<StopTerminalSessionRequest>,
    ) -> Result<Response<StopTerminalSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        let terminal_id = req.terminal_id.trim().to_string();
        if terminal_id == MAIN_TERMINAL_ID {
            return Err(Status::invalid_argument(
                "the main terminal cannot be stopped via StopTerminalSession; \
                 use SignalSession or DeleteSession",
            ));
        }

        if self
            .claude_cli_manager
            .stop_terminal(&session_id, &terminal_id)
            .await
        {
            Ok(Response::new(StopTerminalSessionResponse {
                ok: true,
                message: String::new(),
            }))
        } else {
            Err(Status::not_found("terminal not found"))
        }
    }

    async fn list_terminal_sessions(
        &self,
        request: Request<ListTerminalSessionsRequest>,
    ) -> Result<Response<ListTerminalSessionsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        let terminals = self
            .claude_cli_manager
            .list_terminals(&session_id)
            .await
            .iter()
            .map(|h| TerminalSessionInfo {
                terminal_id: h.terminal_id.clone(),
                kind: h.kind.clone(),
                pid: h.pid,
            })
            .collect();
        Ok(Response::new(ListTerminalSessionsResponse { terminals }))
    }

    async fn remove_worktree(
        &self,
        request: Request<RemoveWorktreeRequest>,
    ) -> Result<Response<RemoveWorktreeResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let project_id = req.project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }
        let worktree_path_raw = req.worktree_path.trim();
        if worktree_path_raw.is_empty() {
            return Err(Status::invalid_argument("worktree_path is required"));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;

        let local_id = local_instance_id_for_config(&self.config);
        let main_repo_str =
            project_storage::main_repo_path_for_host(&projects_dir, project_id, local_id.as_str())
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("project not found"))?;

        let main_repo = PathBuf::from(&main_repo_str);
        if !main_repo.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        let worktree_path = PathBuf::from(worktree_path_raw);

        let repo_blocking = main_repo.clone();
        let wt_blocking = worktree_path.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let join = tokio::task::spawn_blocking(move || {
            worktrees::remove_worktree_under_repo(&repo_blocking, &wt_blocking)
        });

        match tokio::time::timeout(timeout, join).await {
            Ok(Ok(Ok(()))) => {
                self.worktree_stats_cache.invalidate_project(project_id);
                Ok(Response::new(RemoveWorktreeResponse {
                    ok: true,
                    message: String::new(),
                }))
            }
            Ok(Ok(Err(e))) => Err(map_remove_worktree_error(e)),
            Ok(Err(join_err)) => Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => Err(Status::deadline_exceeded(format!(
                "RemoveWorktree: timed out after {}s (spawn_worker_request_timeout_secs)",
                timeout.as_secs()
            ))),
        }
    }

    async fn list_project_branches(
        &self,
        request: Request<ListProjectBranchesRequest>,
    ) -> Result<Response<ListProjectBranchesResponse>, Status> {
        const BRANCH_LIST_LIMIT: usize = 50;

        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let project_id = req.project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        if !repo_root.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        let timeout = self.config.spawn_worker_request_timeout();
        let branches = spawn_blocking_with_timeout(
            timeout,
            "ListProjectBranches: git remote refs",
            move || {
                tddy_core::list_recent_remote_branches(&repo_root, BRANCH_LIST_LIMIT)
                    .map_err(|e| anyhow::anyhow!("list_recent_remote_branches failed: {}", e))
            },
        )
        .await?;

        log::debug!(
            target: "tddy_daemon::connection_service",
            "list_project_branches: project_id={} returned {} branches",
            project_id,
            branches.len()
        );

        Ok(Response::new(ListProjectBranchesResponse { branches }))
    }

    async fn execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<ExecuteToolResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay (which has no local sessions) can forward.
        let requested_daemon = req.daemon_instance_id.trim();
        if !requested_daemon.is_empty() {
            let local_id = local_instance_id_for_config(&self.config);
            let eligible_rows = self.eligible_daemon_source.list_eligible_daemons();
            let eligible_ids: Vec<String> = eligible_rows
                .iter()
                .map(|e| e.instance_id.0.clone())
                .collect();
            match crate::livekit_peer_discovery::classify_peer_route(
                &local_id,
                requested_daemon,
                &eligible_ids,
            ) {
                Err(msg) => {
                    log::info!("ExecuteTool: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    log::info!(
                        "ExecuteTool: forwarding RPC to remote daemon_instance_id={}",
                        peer_instance_id
                    );
                    let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                        Status::failed_precondition(
                            "cannot forward ExecuteTool: this process has no LiveKit common-room connection",
                        )
                    })?;
                    let body = req.encode_to_vec();
                    let out = crate::livekit_peer_discovery::forward_to_peer(
                        slot,
                        &peer_instance_id,
                        "connection.ConnectionService",
                        "ExecuteTool",
                        body,
                    )
                    .await?;
                    let inner = ExecuteToolResponse::decode(out.as_slice()).map_err(|e| {
                        Status::internal(format!("decode ExecuteToolResponse: {e}"))
                    })?;
                    return Ok(Response::new(inner));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Local) => {
                    // Fall through to local execution below.
                }
            }
        }

        // Authenticate caller.
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        // Validate session ID.
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        // Resolve the sessions base and the session's worktree root.
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;

        let worktree_root =
            workspace_session::resolve_worktree_root_for_session(&sessions_base, &req.session_id)?;

        // For path-bearing tools, perform an upfront path traversal check.
        if matches!(
            req.tool_name.as_str(),
            "Read" | "Write" | "StrReplace" | "Delete"
        ) {
            let args_val: serde_json::Value =
                serde_json::from_str(&req.args_json).unwrap_or(serde_json::Value::Null);
            if let Some(path_str) = args_val.get("path").and_then(|v| v.as_str()) {
                // Reject obvious traversal before any I/O.
                let p = std::path::Path::new(path_str);
                if p.components().any(|c| c == std::path::Component::ParentDir) {
                    return Err(Status::permission_denied(
                        "path contains '..' components (traversal rejected)",
                    ));
                }
            }
        }

        // Dispatch.
        let outcome = tool_engine::execute_tool(
            &worktree_root,
            &req.tool_name,
            &req.args_json,
            &self.task_registry,
            &req.session_id,
        )
        .await;

        // Durably record the tool call (non-fatal on failure).
        {
            let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
            let record = crate::tool_call_log::ToolCallRecord {
                task_id: outcome.job_id.clone(),
                tool_name: req.tool_name.clone(),
                args_json: req.args_json.clone(),
                result_json: outcome.result_json.clone(),
                is_error: outcome.is_error,
                error_message: outcome.error_message.clone(),
                job_running: outcome.job_running,
                created_unix_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            };
            if let Err(e) = crate::tool_call_log::append_tool_call(&session_dir, &record) {
                log::warn!(
                    "tool_call_log: failed to persist tool call for session {}: {}",
                    req.session_id,
                    e
                );
            }
        }

        Ok(Response::new(ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        }))
    }

    async fn list_exec_tools(
        &self,
        request: Request<ListExecToolsRequest>,
    ) -> Result<Response<ListExecToolsResponse>, Status> {
        let req = request.into_inner();

        // Route BEFORE auth so a relay (which has no local user table) can forward.
        let requested_daemon = req.daemon_instance_id.trim();
        if !requested_daemon.is_empty() {
            let local_id = local_instance_id_for_config(&self.config);
            let eligible_rows = self.eligible_daemon_source.list_eligible_daemons();
            let eligible_ids: Vec<String> = eligible_rows
                .iter()
                .map(|e| e.instance_id.0.clone())
                .collect();
            match crate::livekit_peer_discovery::classify_peer_route(
                &local_id,
                requested_daemon,
                &eligible_ids,
            ) {
                Err(msg) => {
                    log::info!("ListExecTools: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    log::info!(
                        "ListExecTools: forwarding RPC to remote daemon_instance_id={}",
                        peer_instance_id
                    );
                    let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                        Status::failed_precondition(
                            "cannot forward ListExecTools: this process has no LiveKit common-room connection",
                        )
                    })?;
                    let body = req.encode_to_vec();
                    let out = crate::livekit_peer_discovery::forward_to_peer(
                        slot,
                        &peer_instance_id,
                        "connection.ConnectionService",
                        "ListExecTools",
                        body,
                    )
                    .await?;
                    let inner = ListExecToolsResponse::decode(out.as_slice()).map_err(|e| {
                        Status::internal(format!("decode ListExecToolsResponse: {e}"))
                    })?;
                    return Ok(Response::new(inner));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Local) => {
                    // Fall through to local execution below.
                }
            }
        }

        // Minimal auth — verify caller is a known user.
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        Ok(Response::new(ListExecToolsResponse {
            tools: tool_catalog::tool_catalog(),
        }))
    }

    async fn list_session_tool_calls(
        &self,
        request: Request<ListSessionToolCallsRequest>,
    ) -> Result<Response<ListSessionToolCallsResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay can forward.
        let requested_daemon = req.daemon_instance_id.trim();
        if !requested_daemon.is_empty() {
            let local_id = local_instance_id_for_config(&self.config);
            let eligible_rows = self.eligible_daemon_source.list_eligible_daemons();
            let eligible_ids: Vec<String> = eligible_rows
                .iter()
                .map(|e| e.instance_id.0.clone())
                .collect();
            match crate::livekit_peer_discovery::classify_peer_route(
                &local_id,
                requested_daemon,
                &eligible_ids,
            ) {
                Err(msg) => {
                    log::info!("ListSessionToolCalls: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    log::info!(
                        "ListSessionToolCalls: forwarding RPC to remote daemon_instance_id={}",
                        peer_instance_id
                    );
                    let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                        Status::failed_precondition(
                            "cannot forward ListSessionToolCalls: this process has no LiveKit common-room connection",
                        )
                    })?;
                    let body = req.encode_to_vec();
                    let out = crate::livekit_peer_discovery::forward_to_peer(
                        slot,
                        &peer_instance_id,
                        "connection.ConnectionService",
                        "ListSessionToolCalls",
                        body,
                    )
                    .await?;
                    let inner =
                        ListSessionToolCallsResponse::decode(out.as_slice()).map_err(|e| {
                            Status::internal(format!("decode ListSessionToolCallsResponse: {e}"))
                        })?;
                    return Ok(Response::new(inner));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Local) => {
                    // Fall through to local execution below.
                }
            }
        }

        // Authenticate caller.
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        // Validate session ID segment to prevent path traversal.
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        // Resolve the sessions base path.
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;

        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        let records = crate::tool_call_log::read_tool_calls(&session_dir).unwrap_or_default();

        let tool_calls: Vec<ProtoToolCallInfo> = records
            .into_iter()
            .map(|r| ProtoToolCallInfo {
                task_id: r.task_id,
                tool_name: r.tool_name,
                args_json: r.args_json,
                result_json: r.result_json,
                is_error: r.is_error,
                error_message: r.error_message,
                job_running: r.job_running,
                created_unix_ms: r.created_unix_ms,
            })
            .collect();

        Ok(Response::new(ListSessionToolCallsResponse { tool_calls }))
    }

    async fn report_session_status(
        &self,
        request: Request<ReportSessionStatusRequest>,
    ) -> Result<Response<ReportSessionStatusResponse>, Status> {
        let req = request.into_inner();

        // Validate session_id segment to prevent path traversal.
        tddy_core::validate_session_id_segment(&req.session_id)
            .map_err(|_| Status::invalid_argument("invalid session_id"))?;

        // Validate status string before any IO.
        tddy_core::SessionActivityStatus::from_wire(&req.status)
            .ok_or_else(|| Status::invalid_argument(format!("unknown status: {}", req.status)))?;

        // Resolve sessions_base from os_user (no web session token available for hooks).
        let sessions_base = crate::user_sessions_path::sessions_base_for_user(
            &req.os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::not_found("unknown os_user or sessions_base not found"))?;

        let session_dir = tddy_core::unified_session_dir_path(&sessions_base, &req.session_id);

        // Read session metadata — not found if the directory/yaml doesn't exist.
        let meta = tddy_core::read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        // Only claude-cli sessions support hook status reporting.
        if meta.session_type.as_deref() != Some("claude-cli") {
            return Err(Status::failed_precondition(
                "session_type is not claude-cli",
            ));
        }

        // Validate hook_token (constant-time string comparison acceptable here — local process).
        let stored_token = meta.hook_token.as_deref().unwrap_or("");
        if stored_token != req.hook_token {
            return Err(Status::permission_denied("invalid hook_token"));
        }

        // Persist the activity status.
        tddy_core::update_activity_status(&session_dir, &req.status)
            .map_err(|e| Status::internal(format!("failed to update activity status: {}", e)))?;

        log::debug!(
            target: "tddy_daemon::connection_service",
            "report_session_status: session={} status={}",
            req.session_id,
            req.status
        );

        if let Some(ref telegram) = self.telegram {
            let mut w = telegram.watcher.lock().await;
            w.on_claude_cli_activity_status_changed(
                &*telegram.sender,
                &req.session_id,
                &req.status,
            )
            .await;
        }

        Ok(Response::new(ReportSessionStatusResponse { ok: true }))
    }

    async fn start_demo_vm(
        &self,
        request: Request<StartDemoVmRequest>,
    ) -> Result<Response<StartDemoVmResponse>, Status> {
        let req = request.into_inner();
        self.record_rpc_activity();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        // Read demo-plan.md from the session directory.
        let demo_plan = tddy_workflow_recipes::writer::read_demo_plan_file(&session_dir)
            .map_err(|e| Status::not_found(format!("demo-plan.md not found: {e}")))?;

        let qcow2_path = demo_plan
            .build_target
            .ok_or_else(|| Status::failed_precondition("demo-plan.md has no build_target"))?;
        // ssh_host_port defaults to 2222; the first hostfwd entry is the app port, not SSH.
        let ssh_host_port: u16 = 2222;
        let config = tddy_vm::VmConfig {
            qcow2_path,
            extra_hostfwd: demo_plan
                .hostfwd
                .iter()
                .map(|p| tddy_vm::PortForward {
                    host_port: p.host_port,
                    guest_port: p.guest_port,
                })
                .collect(),
            ssh_host_port,
        };

        // Reject if already booting/running for this session.
        {
            let state = self.demo_vm_state.lock().await;
            if let Some(h) = state.get(&req.session_id) {
                let (state_enum, msg) = match h {
                    DemoVmHandle::Booting => (DemoVmState::Booting, "already booting"),
                    DemoVmHandle::Running { .. } => (DemoVmState::Running, "VM already running"),
                    DemoVmHandle::Error(_) => {
                        // Allow retry after error.
                        return Ok(Response::new(StartDemoVmResponse {
                            state: DemoVmState::Booting as i32,
                            message: "retrying after previous error".to_string(),
                        }));
                    }
                };
                return Ok(Response::new(StartDemoVmResponse {
                    state: state_enum as i32,
                    message: msg.to_string(),
                }));
            }
        }

        // Mark as booting and spawn the boot task.
        {
            let mut state = self.demo_vm_state.lock().await;
            state.insert(req.session_id.clone(), DemoVmHandle::Booting);
        }

        // Build the share URL from the first app hostfwd entry (not the SSH port itself).
        let share_url = config
            .extra_hostfwd
            .first()
            .map(|p| format!("http://localhost:{}", p.host_port))
            .unwrap_or_default();

        let state_ref = Arc::clone(&self.demo_vm_state);
        let session_id = req.session_id.clone();
        tokio::spawn(async move {
            use tddy_vm::Vm as _;
            let vm_impl = tddy_vm::QemuVm;
            match vm_impl.boot(&config).await {
                Ok(vm) => {
                    let mut state = state_ref.lock().await;
                    state.insert(session_id, DemoVmHandle::Running { vm, share_url });
                }
                Err(e) => {
                    let mut state = state_ref.lock().await;
                    state.insert(session_id, DemoVmHandle::Error(e.to_string()));
                }
            }
        });

        log::info!(
            "start_demo_vm: booting VM for session_id={}",
            req.session_id
        );
        Ok(Response::new(StartDemoVmResponse {
            state: DemoVmState::Booting as i32,
            message: "booting".to_string(),
        }))
    }

    async fn stop_demo_vm(
        &self,
        request: Request<StopDemoVmRequest>,
    ) -> Result<Response<StopDemoVmResponse>, Status> {
        let req = request.into_inner();
        self.record_rpc_activity();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let handle = {
            let mut state = self.demo_vm_state.lock().await;
            state.remove(&req.session_id)
        };

        match handle {
            Some(DemoVmHandle::Running { vm, .. }) => {
                use tddy_vm::Vm as _;
                let vm_impl = tddy_vm::QemuVm;
                match vm_impl.shutdown(vm).await {
                    Ok(()) => {
                        log::info!("stop_demo_vm: shutdown ok session_id={}", req.session_id);
                        Ok(Response::new(StopDemoVmResponse {
                            ok: true,
                            message: "shutdown".to_string(),
                        }))
                    }
                    Err(e) => Err(Status::internal(format!("shutdown failed: {e}"))),
                }
            }
            Some(DemoVmHandle::Booting) => Err(Status::failed_precondition(
                "VM is still booting; wait until Running before stopping",
            )),
            Some(DemoVmHandle::Error(msg)) => Ok(Response::new(StopDemoVmResponse {
                ok: true,
                message: format!("VM was in error state ({msg}); cleared"),
            })),
            None => Ok(Response::new(StopDemoVmResponse {
                ok: true,
                message: "no VM running for this session".to_string(),
            })),
        }
    }

    async fn get_demo_vm_status(
        &self,
        request: Request<GetDemoVmStatusRequest>,
    ) -> Result<Response<GetDemoVmStatusResponse>, Status> {
        let req = request.into_inner();
        self.record_rpc_activity();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let state = self.demo_vm_state.lock().await;
        let resp = match state.get(&req.session_id) {
            None => GetDemoVmStatusResponse {
                state: DemoVmState::Stopped as i32,
                ssh_host_port: 0,
                message: "no VM for this session".to_string(),
                share_url: String::new(),
            },
            Some(DemoVmHandle::Booting) => GetDemoVmStatusResponse {
                state: DemoVmState::Booting as i32,
                ssh_host_port: 0,
                message: "booting".to_string(),
                share_url: String::new(),
            },
            Some(DemoVmHandle::Running { vm, share_url }) => GetDemoVmStatusResponse {
                state: DemoVmState::Running as i32,
                ssh_host_port: vm.ssh_host_port as u32,
                message: "running".to_string(),
                share_url: share_url.clone(),
            },
            Some(DemoVmHandle::Error(msg)) => GetDemoVmStatusResponse {
                state: DemoVmState::Error as i32,
                ssh_host_port: 0,
                message: msg.clone(),
                share_url: String::new(),
            },
        };
        Ok(Response::new(resp))
    }

    // --- terminal control mutex ---

    type WatchTerminalControlStream = MpscControlEventStream;

    /// Claim exclusive input control of a session's terminals.
    async fn claim_terminal_control(
        &self,
        request: Request<ClaimTerminalControlRequest>,
    ) -> Result<Response<ClaimTerminalControlResponse>, Status> {
        let req = request.into_inner();
        let _github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let outcome = self
            .claude_cli_manager
            .claim_control(&req.session_id, &req.screen_id, req.steal)
            .await;
        let resp = match outcome {
            ClaimOutcome::Granted { control_token } => ClaimTerminalControlResponse {
                granted: true,
                control_token,
                current_holder_screen_id: String::new(),
            },
            ClaimOutcome::Denied { holder_screen_id } => ClaimTerminalControlResponse {
                granted: false,
                control_token: String::new(),
                current_holder_screen_id: holder_screen_id,
            },
        };
        Ok(Response::new(resp))
    }

    /// Watch for control-lease changes on a session; emits a snapshot immediately, then one event
    /// per lease change.
    async fn watch_terminal_control(
        &self,
        request: Request<WatchTerminalControlRequest>,
    ) -> Result<Response<Self::WatchTerminalControlStream>, Status> {
        let req = request.into_inner();
        let _github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        let session_id = req.session_id.clone();
        let control_token = req.control_token.clone();

        let you_are_controller = self
            .claude_cli_manager
            .verify_control(&session_id, &control_token)
            .await;
        let holder_screen_id = self
            .claude_cli_manager
            .current_control(&session_id)
            .await
            .map(|l| l.holder_screen_id)
            .unwrap_or_default();

        let broadcast_rx = self.claude_cli_manager.subscribe_control();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<TerminalControlEvent>();

        let snapshot = TerminalControlEvent {
            holder_screen_id,
            you_are_controller,
        };
        let _ = tx.send(snapshot);

        let manager = Arc::clone(&self.claude_cli_manager);
        tokio::spawn(relay_control_events(
            session_id,
            control_token,
            manager,
            broadcast_rx,
            tx,
        ));

        Ok(Response::new(MpscControlEventStream { rx }))
    }
}

fn map_remove_worktree_error(e: RemoveWorktreeError) -> Status {
    match e {
        RemoveWorktreeError::NotListed => {
            Status::not_found("worktree path is not in git worktree list")
        }
        RemoveWorktreeError::CannotRemovePrimary => {
            Status::failed_precondition("cannot remove primary worktree")
        }
        RemoveWorktreeError::GitFailed { message } | RemoveWorktreeError::Io(message) => {
            Status::internal(message)
        }
    }
}

#[cfg(test)]
mod signal_session_unit_tests {
    use super::*;
    use tddy_core::session_lifecycle::unified_session_dir_path;
    use tddy_core::SessionMetadata;

    fn make_unit_config() -> crate::config::DaemonConfig {
        let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        crate::config::DaemonConfig::load(&path).unwrap()
    }

    fn make_unit_service(sessions_base: std::path::PathBuf) -> ConnectionServiceImpl {
        let config = make_unit_config();
        let base = sessions_base.clone();
        let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
        let user_resolver: SessionUserResolver = Arc::new(|token| {
            if token == "valid" {
                Some("u".to_string())
            } else {
                None
            }
        });
        ConnectionServiceImpl::new(
            config,
            sessions_base_resolver,
            sessions_base.clone(),
            user_resolver,
            None,
            None,
            None,
            Arc::new(ClaudeCliSessionManager::new()),
        )
    }

    fn write_unit_session(session_dir: &std::path::Path, pid: u32) {
        let session_id = session_dir
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let metadata = SessionMetadata {
            session_id,
            project_id: "proj-unit".to_string(),
            created_at: "2026-03-21T00:00:00Z".to_string(),
            updated_at: "2026-03-21T00:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp".to_string()),
            pid: Some(pid),
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: None,
            model: None,
            activity_status: None,
            hook_token: None,
            sandbox: None,
            agent: None,
            recipe: None,
            specialized_agents: Vec::new(),
        };
        tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
    }

    /// Unit: signal_session rejects an invalid (empty) session token.
    #[tokio::test]
    async fn signal_session_unit_rejects_invalid_token() {
        let temp = tempfile::tempdir().unwrap();
        let service = make_unit_service(temp.path().to_path_buf());
        let request = Request::new(SignalSessionRequest {
            session_token: "bad-token".to_string(),
            session_id: "any".to_string(),
            signal: Signal::Sigint as i32,
            control_token: String::new(),
        });
        let result = service.signal_session(request).await;
        assert!(result.is_err(), "invalid token should return error");
        assert_eq!(result.unwrap_err().code, tddy_rpc::Code::Unauthenticated);
    }

    /// Unit: signal_session returns not-found for a session that has no yaml file.
    #[tokio::test]
    async fn signal_session_unit_returns_error_for_missing_session() {
        let temp = tempfile::tempdir().unwrap();
        let service = make_unit_service(temp.path().to_path_buf());
        let request = Request::new(SignalSessionRequest {
            session_token: "valid".to_string(),
            session_id: "no-such-session".to_string(),
            signal: Signal::Sigterm as i32,
            control_token: String::new(),
        });
        let result = service.signal_session(request).await;
        assert!(result.is_err(), "missing session should return error");
        assert_eq!(result.unwrap_err().code, tddy_rpc::Code::NotFound);
    }

    /// Unit: signal_session with SIGKILL sends correct signal to a live process.
    #[tokio::test]
    async fn signal_session_unit_sigkill_reaches_live_process() {
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();

        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_dir = unified_session_dir_path(&sessions_base, "sigkill-session");
        std::fs::create_dir_all(&session_dir).unwrap();
        write_unit_session(&session_dir, pid);

        let service = make_unit_service(sessions_base);
        let request = Request::new(SignalSessionRequest {
            session_token: "valid".to_string(),
            session_id: "sigkill-session".to_string(),
            signal: Signal::Sigkill as i32,
            control_token: String::new(),
        });
        let response = service.signal_session(request).await.unwrap();
        assert!(response.into_inner().ok);

        let status = child.wait().unwrap();
        assert!(!status.success(), "process should have been killed");
    }
}

#[cfg(test)]
mod delete_session_unit_tests {
    use super::*;
    use tddy_service::proto::connection::DeleteSessionRequest;

    fn make_unit_config() -> crate::config::DaemonConfig {
        let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        crate::config::DaemonConfig::load(&path).unwrap()
    }

    fn make_unit_service(sessions_base: std::path::PathBuf) -> ConnectionServiceImpl {
        let config = make_unit_config();
        let base = sessions_base.clone();
        let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
        let user_resolver: SessionUserResolver = Arc::new(|token| {
            if token == "valid" {
                Some("u".to_string())
            } else {
                None
            }
        });
        ConnectionServiceImpl::new(
            config,
            sessions_base_resolver,
            sessions_base.clone(),
            user_resolver,
            None,
            None,
            None,
            Arc::new(ClaudeCliSessionManager::new()),
        )
    }

    /// Unit: delete_session rejects an invalid session token before touching the filesystem.
    #[tokio::test]
    async fn delete_session_unit_rejects_invalid_token() {
        let temp = tempfile::tempdir().unwrap();
        let service = make_unit_service(temp.path().to_path_buf());
        let request = Request::new(DeleteSessionRequest {
            session_token: "bad-token".to_string(),
            session_id: "any-session".to_string(),
        });
        let result = service.delete_session(request).await;
        assert!(result.is_err(), "invalid token should return error");
        assert_eq!(result.unwrap_err().code, tddy_rpc::Code::Unauthenticated);
    }
}

#[cfg(test)]
mod list_sessions_unit_tests {
    use super::*;
    use std::fs;
    use tddy_core::output::SESSIONS_SUBDIR;
    use tddy_core::{write_session_metadata, SessionMetadata};
    use tddy_service::proto::connection::ListSessionsRequest;

    fn make_unit_config() -> crate::config::DaemonConfig {
        let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        crate::config::DaemonConfig::load(&path).unwrap()
    }

    fn make_unit_service(sessions_base: std::path::PathBuf) -> ConnectionServiceImpl {
        let config = make_unit_config();
        let base = sessions_base.clone();
        let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
        let user_resolver: SessionUserResolver = Arc::new(|token| {
            if token == "valid" {
                Some("u".to_string())
            } else {
                None
            }
        });
        ConnectionServiceImpl::new(
            config,
            sessions_base_resolver,
            sessions_base.clone(),
            user_resolver,
            None,
            None,
            None,
            Arc::new(ClaudeCliSessionManager::new()),
        )
    }

    #[tokio::test]
    async fn list_sessions_unit_returns_new_metadata_fields() {
        let temp = tempfile::tempdir().unwrap();
        let session_id = "list-test-session-001";
        let session_dir = temp.path().join(SESSIONS_SUBDIR).join(session_id);
        fs::create_dir_all(&session_dir).unwrap();

        let metadata = SessionMetadata {
            session_id: session_id.to_string(),
            project_id: "".to_string(),
            created_at: "2026-06-21T10:00:00Z".to_string(),
            updated_at: "2026-06-21T12:00:00Z".to_string(),
            status: "exited".to_string(),
            repo_path: Some("/home/dev/repo".to_string()),
            pid: None,
            tool: Some("tddy-coder".to_string()),
            livekit_room: Some("room-xyz-ct".to_string()),
            pending_elicitation: false,
            previous_session_id: Some("ancestor-session".to_string()),
            session_type: Some("tool".to_string()),
            model: None,
            activity_status: None,
            hook_token: None,
            sandbox: None,
            agent: None,
            recipe: None,
            specialized_agents: Vec::new(),
        };
        write_session_metadata(&session_dir, &metadata).unwrap();

        let service = make_unit_service(temp.path().to_path_buf());
        let result = service
            .list_sessions(Request::new(ListSessionsRequest {
                session_token: "valid".to_string(),
            }))
            .await;
        assert!(result.is_ok());
        let sessions = result.unwrap().into_inner().sessions;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].tool, "tddy-coder");
        assert_eq!(sessions[0].session_type, "tool");
        assert_eq!(sessions[0].updated_at, "2026-06-21T12:00:00Z");
        assert_eq!(sessions[0].previous_session_id, "ancestor-session");
        assert_eq!(sessions[0].livekit_room, "room-xyz-ct");
    }
}

#[cfg(test)]
mod report_session_status_unit_tests {
    use super::*;
    use tddy_core::session_lifecycle::unified_session_dir_path;
    use tddy_core::SessionMetadata;
    use tddy_service::proto::connection::ReportSessionStatusRequest;

    const TEST_HOOK_TOKEN: &str = "tok-unit-hook-abc123";
    const TEST_OS_USER: &str = "u";

    fn make_unit_config() -> crate::config::DaemonConfig {
        let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        crate::config::DaemonConfig::load(&path).unwrap()
    }

    fn make_unit_service(sessions_base: std::path::PathBuf) -> ConnectionServiceImpl {
        let config = make_unit_config();
        let base = sessions_base.clone();
        let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |os_user| {
            if os_user == TEST_OS_USER {
                Some(base.clone())
            } else {
                None
            }
        });
        let user_resolver: SessionUserResolver = Arc::new(|token| {
            if token == "valid" {
                Some("u".to_string())
            } else {
                None
            }
        });
        ConnectionServiceImpl::new(
            config,
            sessions_base_resolver,
            sessions_base,
            user_resolver,
            None,
            None,
            None,
            Arc::new(ClaudeCliSessionManager::new()),
        )
    }

    fn write_claude_cli_session(session_dir: &std::path::Path, hook_token: &str) {
        let session_id = session_dir
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let metadata = SessionMetadata {
            session_id,
            project_id: "proj-hook-unit".to_string(),
            created_at: "2026-06-13T10:00:00Z".to_string(),
            updated_at: "2026-06-13T10:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/worktrees/hook-test".to_string()),
            pid: None,
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some("claude-sonnet-4-6".to_string()),
            activity_status: None,
            hook_token: Some(hook_token.to_string()),
            sandbox: None,
            agent: None,
            recipe: None,
            specialized_agents: Vec::new(),
        };
        tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
    }

    /// Happy path: valid hook_token, claude-cli session, known status → activity_status written
    /// to `.session.yaml`.
    #[tokio::test]
    async fn report_session_status_writes_activity_status_to_session_yaml() {
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "hook-writes-status-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);

        let service = make_unit_service(sessions_base);
        let request = Request::new(ReportSessionStatusRequest {
            session_id: session_id.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: TEST_OS_USER.to_string(),
            status: "Running".to_string(),
        });
        let response = service.report_session_status(request).await.unwrap();
        assert!(response.into_inner().ok, "ok must be true on success");

        let meta = tddy_core::read_session_metadata(&session_dir).unwrap();
        assert_eq!(
            meta.activity_status.as_deref(),
            Some("Running"),
            "activity_status must be written to .session.yaml"
        );
    }

    /// Missing session → NotFound.
    #[tokio::test]
    async fn report_session_status_rejects_unknown_session() {
        let temp = tempfile::tempdir().unwrap();
        let service = make_unit_service(temp.path().to_path_buf());
        let request = Request::new(ReportSessionStatusRequest {
            session_id: "no-such-session".to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: TEST_OS_USER.to_string(),
            status: "Running".to_string(),
        });
        let err = service.report_session_status(request).await.unwrap_err();
        assert_eq!(err.code, tddy_rpc::Code::NotFound);
    }

    /// Wrong hook_token → PermissionDenied.
    #[tokio::test]
    async fn report_session_status_rejects_bad_hook_token() {
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "hook-bad-token-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);

        let service = make_unit_service(sessions_base);
        let request = Request::new(ReportSessionStatusRequest {
            session_id: session_id.to_string(),
            hook_token: "wrong-token".to_string(),
            os_user: TEST_OS_USER.to_string(),
            status: "Running".to_string(),
        });
        let err = service.report_session_status(request).await.unwrap_err();
        assert_eq!(err.code, tddy_rpc::Code::PermissionDenied);
    }

    /// Non-claude-cli session (tool session) → FailedPrecondition.
    #[tokio::test]
    async fn report_session_status_rejects_non_claude_cli_session() {
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "hook-non-cli-session-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();

        // Tool session — no session_type = "claude-cli", no hook_token.
        let metadata = SessionMetadata {
            session_id: session_id.to_string(),
            project_id: "proj-hook-unit".to_string(),
            created_at: "2026-06-13T10:00:00Z".to_string(),
            updated_at: "2026-06-13T10:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: None,
            pid: Some(99999),
            tool: Some("tddy-coder".to_string()),
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: None,
            model: None,
            activity_status: None,
            hook_token: None,
            sandbox: None,
            agent: None,
            recipe: None,
            specialized_agents: Vec::new(),
        };
        tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();

        let service = make_unit_service(sessions_base);
        let request = Request::new(ReportSessionStatusRequest {
            session_id: session_id.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: TEST_OS_USER.to_string(),
            status: "Running".to_string(),
        });
        let err = service.report_session_status(request).await.unwrap_err();
        assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
    }

    /// Unknown status string (not in the known set) → InvalidArgument.
    #[tokio::test]
    async fn report_session_status_rejects_unknown_status_string() {
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "hook-bad-status-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);

        let service = make_unit_service(sessions_base);
        let request = Request::new(ReportSessionStatusRequest {
            session_id: session_id.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: TEST_OS_USER.to_string(),
            status: "UnknownBadStatus".to_string(),
        });
        let err = service.report_session_status(request).await.unwrap_err();
        assert_eq!(err.code, tddy_rpc::Code::InvalidArgument);
    }

    /// Path-traversal in session_id (`../../etc`) → InvalidArgument before any IO.
    #[tokio::test]
    async fn report_session_status_rejects_session_id_path_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let service = make_unit_service(temp.path().to_path_buf());
        let request = Request::new(ReportSessionStatusRequest {
            session_id: "../../etc/passwd".to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: TEST_OS_USER.to_string(),
            status: "Running".to_string(),
        });
        let err = service.report_session_status(request).await.unwrap_err();
        assert_eq!(err.code, tddy_rpc::Code::InvalidArgument);
    }
}

/// Resuming a session must relaunch its child with the *same* coding agent and workflow recipe it
/// was originally started with — read back from the persisted `.session.yaml`. Before this,
/// `ResumeSession` hard-coded `agent: None` / `recipe: None`, so tddy-coder fell back to its
/// default agent (`claude`), turning a resumed `cursor` / `pr-stack` session into a broken
/// `claude --resume <foreign-id>` run.
#[cfg(test)]
mod resume_agent_recipe_restore_tests {
    use super::resume_agent_and_recipe;
    use tddy_core::SessionMetadata;

    fn metadata_from_yaml(yaml: &str) -> SessionMetadata {
        serde_yaml::from_str(yaml).expect("test metadata YAML must deserialize into SessionMetadata")
    }

    #[test]
    fn resume_restores_the_sessions_persisted_agent_and_recipe() {
        // Given a persisted cursor / pr-stack session
        let metadata = metadata_from_yaml(
            r#"session_id: 019f243a-8e31-7203-81dd-53f5ef8b9352
project_id: proj-prstack
created_at: "2026-07-02T19:07:25Z"
updated_at: "2026-07-02T19:07:25Z"
status: active
agent: cursor
recipe: pr-stack
"#,
        );

        // When the daemon derives the spawn's agent and recipe for a resume
        let (agent, recipe) = resume_agent_and_recipe(&metadata);

        // Then the child is relaunched with the original agent and recipe, not the default claude
        assert_eq!(
            agent.as_deref(),
            Some("cursor"),
            "resume must restore the session's original agent, not fall back to default claude"
        );
        assert_eq!(
            recipe.as_deref(),
            Some("pr-stack"),
            "resume must restore the session's original recipe"
        );
    }

    #[test]
    fn resume_of_a_legacy_session_without_persisted_agent_yields_none() {
        // Given a legacy session that predates agent/recipe persistence
        let metadata = metadata_from_yaml(
            r#"session_id: legacy-sess
project_id: proj-legacy
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
status: active
"#,
        );

        // When the daemon derives the spawn's agent and recipe for a resume
        let (agent, recipe) = resume_agent_and_recipe(&metadata);

        // Then there is nothing to restore (tddy-coder applies its own resolution downstream)
        assert!(agent.is_none(), "legacy session has no persisted agent to restore");
        assert!(recipe.is_none(), "legacy session has no persisted recipe to restore");
    }
}

#[cfg(test)]
mod specialized_subagent_env_unit_tests {
    //! Unit tests: `ConnectionServiceImpl::specialized_subagent_env` — resolving
    //! `StartSessionRequest.specialized_agents` names into the `TDDY_SUBAGENT`/
    //! `TDDY_SUBAGENTS_JSON` jail env pair.
    //!
    //! Feature: docs/ft/coder/specialized-subagents.md (criteria 17-18)
    //! Changeset: docs/dev/1-WIP/specialized-subagents.md
    //!
    //! The full sandboxed spawn (`start_sandboxed_claude_cli_session`) requires a real git
    //! repo/project/platform sandbox (darwin Seatbelt / Linux cgroups) — see
    //! `sandboxed_claude_cli_acceptance.rs` for that heavier end-to-end harness. This module
    //! isolates the new, platform-independent resolution logic this changeset adds.

    use super::*;

    fn make_unit_config() -> crate::config::DaemonConfig {
        let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        crate::config::DaemonConfig::load(&path).unwrap()
    }

    fn make_unit_service(tddy_data_dir: std::path::PathBuf) -> ConnectionServiceImpl {
        let config = make_unit_config();
        let base = tddy_data_dir.clone();
        let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
        let user_resolver: SessionUserResolver = Arc::new(|token| {
            if token == "valid" {
                Some("u".to_string())
            } else {
                None
            }
        });
        ConnectionServiceImpl::new(
            config,
            sessions_base_resolver,
            tddy_data_dir,
            user_resolver,
            None,
            None,
            None,
            Arc::new(ClaudeCliSessionManager::new()),
        )
    }

    /// An empty `specialized_agents` list is never consulted by the caller (see the `if
    /// !specialized_defs.is_empty()` guard in `start_sandboxed_claude_cli_session`) — this test
    /// documents that `specialized_subagent_env` itself, when called directly with an empty def
    /// list, still resolves cleanly (an empty env pair list), matching "no subagents requested =
    /// no subagent env vars" rather than an error.
    #[test]
    fn specialized_subagent_env_with_no_defs_produces_no_env_pairs() {
        // Given
        let tddy_home = tempfile::tempdir().unwrap();
        let service = make_unit_service(tddy_home.path().to_path_buf());

        // When
        let result = service.specialized_subagent_env(&[]);

        // Then
        assert_eq!(
            result.unwrap(),
            Vec::<(String, String)>::new(),
            "an empty defs list must resolve to no env pairs, not an error"
        );
    }

    /// An empty `specialized_agents` name list resolves to an empty defs list, not an error.
    #[test]
    fn resolve_specialized_agent_defs_with_no_names_produces_no_defs() {
        // Given
        let tddy_home = tempfile::tempdir().unwrap();
        let service = make_unit_service(tddy_home.path().to_path_buf());

        // When
        let result = service.resolve_specialized_agent_defs(&[]);

        // Then
        assert_eq!(
            result.unwrap(),
            Vec::<tddy_discovery::agent_def::SpecializedAgentDef>::new(),
            "an empty specialized_agents list must resolve to no defs, not an error"
        );
    }

    /// A single resolvable name (the always-available builtin `fastcontext`) resolves to that
    /// def — no `<tddyhome>/agents` override needed.
    #[test]
    fn resolve_specialized_agent_defs_resolves_the_builtin_fastcontext_name() {
        // Given — no <tddyhome>/agents overrides; "fastcontext" must still resolve via the
        // builtin def
        let tddy_home = tempfile::tempdir().unwrap();
        let service = make_unit_service(tddy_home.path().to_path_buf());

        // When
        let result = service.resolve_specialized_agent_defs(&["fastcontext".to_string()]);

        // Then
        let defs = result.expect("fastcontext must resolve via the builtin def");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "fastcontext");
    }

    /// A name that resolves against neither the builtins nor `<tddyhome>/agents` must reject the
    /// whole request — no partial resolution for the names that *did* resolve.
    #[test]
    fn resolve_specialized_agent_defs_rejects_unknown_name() {
        // Given
        let tddy_home = tempfile::tempdir().unwrap();
        let service = make_unit_service(tddy_home.path().to_path_buf());

        // When
        let result = service.resolve_specialized_agent_defs(&[
            "fastcontext".to_string(),
            "ghost-agent".to_string(),
        ]);

        // Then
        let err = result.expect_err("an unresolvable name must reject the whole request");
        assert_eq!(err.code(), tddy_rpc::Code::InvalidArgument);
        assert!(
            err.message().contains("ghost-agent"),
            "the error must name the unresolvable subagent; got: {}",
            err.message()
        );
    }

    /// A resolved `fastcontext` def produces both `TDDY_SUBAGENT` (comma names) and
    /// `TDDY_SUBAGENTS_JSON` (the serialized def) — the exact env shape `tddy-tools --mcp` (see
    /// `subagents_from_env` in `tddy-tools/src/server.rs`) expects.
    #[test]
    fn specialized_subagent_env_builds_env_pairs_for_a_resolved_def() {
        // Given
        let tddy_home = tempfile::tempdir().unwrap();
        let service = make_unit_service(tddy_home.path().to_path_buf());
        let defs = service
            .resolve_specialized_agent_defs(&["fastcontext".to_string()])
            .expect("fastcontext must resolve via the builtin def");

        // When
        let result = service.specialized_subagent_env(&defs);

        // Then
        let env = result.expect("a resolved def must build env pairs without error");
        let names = env
            .iter()
            .find(|(k, _)| k == "TDDY_SUBAGENT")
            .map(|(_, v)| v.clone());
        assert_eq!(names.as_deref(), Some("fastcontext"));
        let defs_json = env
            .iter()
            .find(|(k, _)| k == "TDDY_SUBAGENTS_JSON")
            .map(|(_, v)| v.clone())
            .expect("TDDY_SUBAGENTS_JSON must be present");
        assert!(
            defs_json.contains("fastcontext"),
            "TDDY_SUBAGENTS_JSON must serialize the resolved fastcontext def; got: {defs_json}"
        );
    }
}
