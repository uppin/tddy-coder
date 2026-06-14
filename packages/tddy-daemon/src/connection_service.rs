//! ConnectionService implementation for daemon session/tool management.

use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    AgentInfo, ConnectSessionRequest, ConnectSessionResponse,
    ConnectionService as ConnectionServiceTrait, CreateProjectRequest, CreateProjectResponse,
    DeleteSessionRequest, DeleteSessionResponse, EligibleDaemonEntry, ListAgentsRequest,
    ListAgentsResponse, ListEligibleDaemonsRequest, ListEligibleDaemonsResponse,
    ListProjectBranchesRequest, ListProjectBranchesResponse, ListProjectsRequest,
    ListProjectsResponse, ListSessionWorkflowFilesRequest, ListSessionWorkflowFilesResponse,
    ListSessionsRequest, ListSessionsResponse, ListToolsRequest, ListToolsResponse,
    ListWorktreesForProjectRequest, ListWorktreesForProjectResponse,
    ProjectEntry as ProtoProjectEntry, ReadSessionWorkflowFileRequest,
    ReadSessionWorkflowFileResponse, RemoveWorktreeRequest, RemoveWorktreeResponse,
    ReportSessionStatusRequest, ReportSessionStatusResponse, ResumeSessionRequest,
    ResumeSessionResponse, SendTerminalInputResponse, SessionEntry as ProtoSessionEntry,
    SessionTerminalInput, SessionTerminalOutput, Signal, SignalSessionRequest,
    SignalSessionResponse, StartSessionRequest, StartSessionResponse, StreamTerminalOutputRequest,
    ToolInfo, WorkflowFileEntry, WorktreeRow,
};
use uuid::Uuid;

use crate::agent_list_mapping::agent_allowlist_rows;
use crate::claude_cli_session::ClaudeCliSessionManager;
use crate::config::DaemonConfig;
use crate::livekit_peer_discovery::{local_instance_id_for_config, LiveKitDiscoveryHandles};
use crate::multi_host::{EligibleDaemonSource, StubEligibleDaemonSource};
use crate::project_storage::{self, ProjectData};
use crate::session_deletion;
use crate::session_list_enrichment;
use crate::session_reader;
use crate::shell_job_registry::ShellJobRegistry;
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
    ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest, ListExecToolsResponse,
};

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

/// ConnectionService implementation.
pub struct ConnectionServiceImpl {
    config: DaemonConfig,
    sessions_base_for_user: SessionsBaseResolver,
    user_resolver: SessionUserResolver,
    spawn_client: Option<Arc<spawn_worker::SpawnClient>>,
    eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    /// When set, LiveKit **Room** handle for forwarding **StartSession** to peer daemons in `common_room`.
    common_room_livekit_room: Option<Arc<tokio::sync::RwLock<Option<Arc<Room>>>>>,
    telegram: Option<Arc<TelegramDaemonHooks>>,
    worktree_stats_cache: Arc<WorktreeStatsCache>,
    claude_cli_manager: Arc<ClaudeCliSessionManager>,
    /// Registry for background shell jobs spawned by the `Shell` tool (block_until_ms=0).
    shell_jobs: Arc<ShellJobRegistry>,
    /// Optional idle-timeout tracker for relay mode — bumped on every RPC call.
    idle_tracker: Option<Arc<crate::relay_idle::IdleTimeoutTracker>>,
}

impl ConnectionServiceImpl {
    pub fn new(
        config: DaemonConfig,
        sessions_base_for_user: SessionsBaseResolver,
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
            worktrees::projects_stats_cache_root(),
        ));
        let shell_jobs = Arc::new(ShellJobRegistry::new());
        Self {
            config,
            sessions_base_for_user,
            user_resolver,
            spawn_client,
            eligible_daemon_source,
            common_room_livekit_room,
            telegram,
            worktree_stats_cache,
            claude_cli_manager,
            shell_jobs,
            idle_tracker: None,
        }
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
        let projects_dir = projects_path_for_user(os_user)
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
            ..Changeset::default()
        };
        tddy_core::write_changeset(&session_dir, &cs)
            .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;

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
                    None,
                )
                .map_err(|e| anyhow::anyhow!("worktree setup failed: {}", e))
            },
        )
        .await?;

        // Resolve tddy-tools path: config → current_exe sibling → PATH fallback.
        let tddy_tools_path = self
            .config
            .claude_cli
            .as_ref()
            .and_then(|c| c.tddy_tools_path.as_deref())
            .map(|s| s.to_string())
            .or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("tddy-tools")))
                    .map(|p| p.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "tddy-tools".to_string());

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

    /// Handle `ResumeSession` for `session_type = "claude-cli"` sessions.
    async fn resume_claude_cli_session(
        &self,
        session_id: &str,
        session_dir: PathBuf,
        meta: tddy_core::SessionMetadata,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
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
        let sessions_base = (self.sessions_base_for_user)(os_user)
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
        let projects_dir = projects_path_for_user(os_user)
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

        let projects_dir = projects_path_for_user(os_user)
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
            let sessions_base = (self.sessions_base_for_user)(os_user)
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            let session_id = Uuid::now_v7().to_string();
            let timeout = self.config.spawn_worker_request_timeout();
            return workspace_session::start_workspace_session(
                os_user,
                &session_id,
                sessions_base,
                req.project_id.trim(),
                timeout,
            )
            .await;
        }

        // --- claude-cli branch: no LiveKit; resolves project and creates a real git worktree ---
        if req.session_type.trim() == "claude-cli" {
            let sessions_base = (self.sessions_base_for_user)(os_user)
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            let session_id = Uuid::now_v7().to_string();
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
                )
                .await;
        }

        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| Status::failed_precondition("LiveKit not configured"))?;

        let project_id_req = req.project_id.trim();
        if project_id_req.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }

        let projects_dir = projects_path_for_user(os_user)
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
            if let Some(ref client) = spawn_client {
                let spawn_req = spawn_worker::build_spawn_request(
                    &os_user,
                    &tool_path,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: None,
                        new_session_id: None,
                        project_id: pid,
                        agent,
                        mouse: spawn_mouse,
                        recipe,
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
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: None,
                        new_session_id: None,
                        project_id: pid,
                        agent,
                        mouse: spawn_mouse,
                        recipe,
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
        let sessions_base = (self.sessions_base_for_user)(os_user)
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
        let sessions_base = (self.sessions_base_for_user)(os_user)
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        // --- claude-cli branch: resume without LiveKit ---
        if metadata.session_type.as_deref() == Some("claude-cli") {
            return self
                .resume_claude_cli_session(&req.session_id, session_dir, metadata)
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
        let tool_path = metadata.tool.as_deref().unwrap_or("tddy-coder").to_string();
        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| Status::failed_precondition("LiveKit not configured"))?;
        let spawn_client = self.spawn_client.clone();
        let spawn_mouse = self.config.spawn_mouse;
        let os_user = os_user.to_string();
        let session_id = req.session_id.clone();
        let livekit = livekit.clone();
        let project_id_resume = metadata.project_id.clone();
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
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: Some(session_id.as_str()),
                        new_session_id: None,
                        project_id: pid,
                        agent: None,
                        mouse: spawn_mouse,
                        recipe: None,
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
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: Some(session_id.as_str()),
                        new_session_id: None,
                        project_id: pid,
                        agent: None,
                        mouse: spawn_mouse,
                        recipe: None,
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
        let sessions_base = (self.sessions_base_for_user)(os_user)
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
        let sessions_base = (self.sessions_base_for_user)(os_user)
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        log::debug!(
            "DeleteSession: resolved sessions_base={:?} for os_user={}",
            sessions_base,
            os_user
        );
        let projects_dir_opt = projects_path_for_user(os_user);
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
        let sessions_base = (self.sessions_base_for_user)(os_user)
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
        let sessions_base = (self.sessions_base_for_user)(os_user)
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

        let projects_dir = projects_path_for_user(os_user)
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
        log::info!(
            target: "tddy_daemon::connection_service",
            "stream_session_terminal_io: session_id={}",
            session_id
        );
        let handle = self
            .claude_cli_manager
            .get(&session_id)
            .await
            .ok_or_else(|| {
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "stream_session_terminal_io: session {} not found in registry",
                    session_id
                );
                Status::not_found("claude-cli session not found or not running")
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
        tokio::spawn(async move {
            while let Some(Ok(msg)) = in_stream.next().await {
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
        log::info!(
            target: "tddy_daemon::connection_service",
            "stream_terminal_output: session_id={}",
            session_id
        );
        let handle = self
            .claude_cli_manager
            .get(&session_id)
            .await
            .ok_or_else(|| {
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "stream_terminal_output: session {} not found in registry",
                    session_id
                );
                Status::not_found("claude-cli session not found or not running")
            })?;

        // Subscribe to broadcast BEFORE reading the capture buffer so there is no gap:
        // bytes produced between the capture snapshot and the first bridge recv() are
        // covered by the broadcast subscription.
        let (mpsc_tx, mpsc_rx) = tokio::sync::mpsc::unbounded_channel::<bytes::Bytes>();
        let mut broadcast_rx = handle.stdout_tx.subscribe();

        // Replay capture buffer first — this contains all PTY output since session start,
        // including the initial TUI render that arrived before the browser subscribed.
        // The broadcast channel cannot buffer for receivers that didn't exist yet
        // (send() silently fails with 0 receivers), so the capture is the only way to see
        // historical output.
        {
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

        // Trigger a redraw so if the initial TUI render was before session start logging, we
        // get a fresh frame regardless. (The capture replay above already covers the common
        // case, but SIGWINCH is a cheap belt-and-suspenders.)
        handle.trigger_redraw();

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
        let handle = self
            .claude_cli_manager
            .get(&session_id)
            .await
            .ok_or_else(|| Status::not_found("claude-cli session not found or not running"))?;

        if !req.data.is_empty() {
            log::trace!(
                target: "tddy_daemon::connection_service",
                "send_terminal_input: session_id={} {} bytes: {:?}",
                session_id,
                req.data.len(),
                String::from_utf8_lossy(&req.data)
            );
            let _ = handle.stdin_tx.send(bytes::Bytes::from(req.data));
        }
        Ok(Response::new(SendTerminalInputResponse {}))
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

        let projects_dir = projects_path_for_user(os_user)
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

        let projects_dir = projects_path_for_user(os_user)
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
        let sessions_base = (self.sessions_base_for_user)(os_user)
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
            &self.shell_jobs,
        )
        .await;

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
        let sessions_base = (self.sessions_base_for_user)(&req.os_user)
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
