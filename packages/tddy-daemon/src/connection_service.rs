//! ConnectionService implementation for daemon session/tool management.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::stream::Stream;
use livekit::prelude::Room;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::session_lifecycle::validate_session_id_segment;
use tddy_core::Changeset;
use tddy_rpc::{Response, Status};
use tddy_service::proto::catalog::{ListAgentModelsResponse, ModelInfo as CatalogModelInfo};
use tddy_service::proto::connection::{
    start_session_event::Event as StartSessionEventKind, AttachmentMaterializationProgress,
    SessionAttachment, StartSessionEvent,
};
use tddy_service::proto::connection::{
    ProjectEntry as ProtoProjectEntry, SplitAgentPlacement, StartSessionResponse,
};
use uuid::Uuid;

use crate::branch_intent::{
    resolve_branch_workflow, BranchIntentPolicy, BranchIntentRequest, ResolvedBranchWorkflow,
};
use crate::cli_session_manager::CliSessionManager;
use crate::config::DaemonConfig;
use crate::multi_host::EligibleDaemonSource;
use crate::project_storage::{self};
use crate::telegram_session_subscriber::TelegramDaemonHooks;
use crate::user_sessions_path::projects_path_for_user;
use crate::workspace_session;
use tddy_daemon_livekit::livekit_rooms_stream::RoomRoster;
use tddy_daemon_livekit::session_room::ActivityDelta;
use tddy_service::proto::activity::AgentActivityDeltaChunk;
use tddy_service::proto::connection::{ExecuteToolChunk, ExecuteToolResponse};
use tddy_spawn::spawn_worker;
use tddy_spawn::spawner::{self};
use tddy_task::TaskRegistry;

// Bound for the extracted test modules, which reach the code under test through `use super::*`.
// The lib itself no longer names any of these — every user moved into `connection_service/` — so a
// plain `use` would be an unused import there. `#[cfg(test)]` keeps them out of the lib build
// entirely rather than trading a resolution error for a lint.
#[cfg(test)]
#[cfg(test)]
use crate::livekit_peer_discovery::local_instance_id_for_config;
#[cfg(test)]
#[cfg(test)]
#[cfg(test)]
use tddy_core::session_lifecycle::unified_session_dir_path;
#[cfg(test)]
use tddy_rpc::Request;
#[cfg(test)]
use tddy_service::proto::connection::ConnectionService as ConnectionServiceTrait;
#[cfg(test)]
use tddy_service::proto::connection::{
    ExecuteToolRequest, ListProjectsRequest, Signal, SignalSessionRequest, StartSessionRequest,
};

use tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES;

mod service_util;
pub(crate) use service_util::*;

/// Stream adapter backed by an unbounded mpsc channel carrying `Result<T, Status>` items — used for
/// server-streaming RPCs (e.g. `StreamExecuteTool`) whose frames may carry a mid-stream status.
pub struct MpscResultStream<T> {
    rx: tokio::sync::mpsc::UnboundedReceiver<Result<T, Status>>,
}

impl<T> Stream for MpscResultStream<T> {
    type Item = Result<T, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl<T> Unpin for MpscResultStream<T> {}

/// Opaque by design: a stream's pending items are not inspectable without consuming them, so this
/// only names the adapter — enough for a `Result::expect_err` message on a handler that returns it.
impl<T> std::fmt::Debug for MpscResultStream<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MpscResultStream")
    }
}

/// Cadence at which a `StreamSessionAgents` subscription re-sends the roster it last sent, when
/// nothing has been published in the meantime.
///
/// A roster nobody is changing produces no frames for hours, and a *forwarded* subscription — the
/// split session's case, where the agent runs on one daemon and its roster lives on another — rides
/// a relay that terminates a stream which stops producing
/// ([`crate::livekit_peer_discovery::PEER_FORWARD_STREAM_IDLE_TIMEOUT`]). That deadline is
/// deliberate: a stream that goes quiet must never read as one that ended, or a truncated forward
/// passes for a complete one. So the roster has to keep talking. Re-sending the applied revision is
/// the cheapest frame there is — applying a revision already held is a defined no-op that announces
/// no change — and at this cadence two of them fit inside the deadline, so one lost frame does not
/// end the subscription.
pub const ROSTER_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(8);

// The "two of them fit inside the deadline" above is the whole reason this cadence is safe, and it
// is a relation between two constants in two modules — exactly the kind that is edited on one side.
// Checked here rather than in a test so raising either one cannot compile.
const _: () = assert!(
    ROSTER_KEEPALIVE_INTERVAL.as_millis() * 2
        < crate::livekit_peer_discovery::PEER_FORWARD_STREAM_IDLE_TIMEOUT.as_millis(),
    "two roster keepalives must fit inside a relay's idle deadline, or one lost frame ends a \
     forwarded subscription"
);

mod activity_hub;

/// ConnectionService implementation.
///
/// `Clone` is a shallow, shared clone: every mutable field is behind an `Arc`, so a clone talks to
/// the same session managers, registries and caches. The server-streaming handlers need it — they
/// hand the work to a `tokio::spawn`ed producer task, which must own a `'static` service.
#[derive(Clone)]
pub struct ConnectionServiceImpl {
    config: DaemonConfig,
    #[allow(dead_code)]
    // Kept for API compatibility; callers pass a resolver but tddy_data_dir is used directly.
    sessions_base_for_user: tddy_daemon_kernel::SessionsBaseResolver,
    tddy_data_dir: PathBuf,
    user_resolver: tddy_daemon_kernel::SessionUserResolver,
    spawn_client: Option<Arc<spawn_worker::SpawnClient>>,
    eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    /// When set, LiveKit **Room** handle for forwarding **StartSession** to peer daemons in `common_room`.
    common_room_livekit_room: Option<Arc<tokio::sync::RwLock<Option<Arc<Room>>>>>,
    telegram: Option<Arc<TelegramDaemonHooks>>,
    claude_cli_manager: Arc<CliSessionManager>,
    /// Sandboxed claude-cli sessions (darwin Seatbelt).
    sandbox_manager: Arc<tddy_daemon_sandbox::sandbox_session::SandboxSessionManager>,
    /// The per-session jails sandboxed `workspace` sessions dispatch their tools through
    /// (`docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox). Keyed by session id, and
    /// shared across clones so the jail a start provisioned is the one the next handler's
    /// `ExecuteTool` finds.
    workspace_sandboxes: Arc<tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxRegistry>,
    /// What builds those jails. Injected the way `host_stats` and `room_roster` are, so the
    /// dispatch, refusal and ordering contracts are testable without booting one.
    workspace_sandbox_provisioner:
        Arc<dyn tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxProvisioner>,
    /// Registry for Tasks created by tool invocations (every ExecuteTool call).
    task_registry: TaskRegistry,
    /// Optional idle-timeout tracker for relay mode — bumped on every RPC call.
    idle_tracker: Option<Arc<crate::relay_idle::IdleTimeoutTracker>>,
    /// Reader for the LiveKit server's rooms and their participants. `StreamLiveKitRooms` left for
    /// `livekit.LiveKitService`; what still reads the roster here is agent-clone provisioning,
    /// which needs to know whether a session's room already exists.
    room_roster: Arc<dyn RoomRoster>,
    /// Cadence at which a `StreamSessionAgents` subscription re-sends an unchanged roster
    /// (overridable for tests).
    roster_keepalive_interval: Duration,
    /// Per-session demo VM state — keyed by session_id.
    demo_vm_state:
        Arc<tokio::sync::Mutex<std::collections::HashMap<String, activity_hub::DemoVmHandle>>>,
    /// Per-session reverse stdio RPC endpoint to a spawned tddy-coder child (grill-me), keyed by
    /// session_id. Hosts [`crate::host_session_service::HostSessionService`] so the coder can relay
    /// `spawn_conversation` back to the daemon over the pipe. Kept alive for the session's lifetime.
    session_stdio: Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<String, seeded_clone_guard::SessionStdioEndpoint>,
        >,
    >,
    /// Live pub/sub hub for agent-activity records (StreamSessionActivity) plus the PreToolUse /
    /// PostToolUse pending-call pairing state. Shared with the sandbox tool handler so both the
    /// hook path and the in-jail tool path publish through the same channel.
    agent_activity_hub: Arc<tddy_daemon_kernel::AgentActivityHub>,
    /// What each agent session's own conversation says its agent is doing
    /// (`docs/ft/daemon/agent-session-status.md`), which `ListSessions` reports. Beside the hub it
    /// subscribes to, and shared across clones so the seed a listing paid for is not re-read by the
    /// next one.
    session_agent_inference: Arc<crate::session_agent_inference::SessionAgentInferenceStore>,
    /// GitHub access tokens retained at web login, keyed by GitHub login — the credential the
    /// PR-status reads act with. `None` (no `auth_storage` configured) means a real login's PR
    /// status reads as *unavailable*, never as "no PR".
    github_token_store: Option<Arc<dyn tddy_github::token_store::GitHubTokenStore>>,
    /// Base of the pre-session attachment staging area; each caller's root is
    /// `{staging_base_dir}/{os_user}/`. Separate from `tddy_data_dir` so an abandoned batch is
    /// cleared by the host restart rather than living in the data dir forever. Defaults to
    /// [`crate::session_attachment_staging::default_staging_base_dir`].
    staging_base_dir: PathBuf,
    /// The per-worktree LiveKit rooms this daemon hosts, keyed by the session owning each checkout
    /// (`docs/ft/daemon/session-room.md`). Holding the joined participant
    /// here is what keeps a room open past the `StartSession` that created it; `DeleteSession`
    /// closes it again.
    session_rooms: Arc<tddy_daemon_livekit::session_room::SessionRoomRegistry>,
    /// This daemon's model registry, whose assistants are selectable agents alongside the
    /// `allowed_agents` config entries. `None` means no registry is wired (a test fixture), in
    /// which case `ListAgents` reports the config entries alone.
    model_registry: Option<Arc<tddy_model_registry::ModelRegistryStore>>,
    /// The agent roster of every session this daemon facilitates
    /// (`docs/ft/daemon/session-agent-roster.md`). Shared across clones so an attach made on one
    /// handler is the roster the next handler — and every `StreamSessionAgents` subscriber — sees.
    session_agent_rosters: Arc<crate::session_agent_roster::SessionAgentRosterStore>,
    /// The checkouts this daemon asked peers to build for its sessions' remote agents. Read by
    /// every roster snapshot, so an entry reports the state of the clone actually serving it.
    session_agent_clones: Arc<crate::session_agent_clone::SessionAgentCloneStore>,
    /// The checkouts this daemon holds on *other* daemons' behalf — the other half of the same
    /// feature, and deliberately a separate map: a daemon is routinely both, and merging the two
    /// would let a clone this daemon hosts answer a question about one it commissioned.
    hosted_agent_clones: Arc<crate::session_agent_clone::HostedAgentClones>,
    /// The per-session registry of owning daemons this daemon (as the facilitating daemon) has
    /// admitted to its session rooms — the room-admission handshake (PRD § "What attach does"
    /// step 3). The admission RPC refreshes entries here; the attach path records the first admit;
    /// the detach path revokes an owning daemon when its last agent in a session goes; the
    /// session-delete path revokes every owning daemon a session admitted at once.
    session_admissions: Arc<crate::session_admission_service::SessionAdmissionRegistry>,
    /// Open conversations with roster agents, keyed by conversation id. Local entries hold a live
    /// turn loop here; remote entries hold only the routing, because the loop runs on the owning
    /// daemon.
    agent_conversations: Arc<tddy_session_agents::OpenAgentConversations>,
    /// Where this daemon publishes its session notifications
    /// (`docs/ft/daemon/session-notifications.md`). `None` means
    /// nothing is listening: publishing is skipped, and `StreamSessionNotifications` has no feed to
    /// hand a client.
    session_notification_bus: Option<Arc<crate::session_notifications::SessionNotificationBus>>,
    /// Self-reference for handing out `Arc<ConnectionServiceImpl>` from a `&self` method. Set
    /// once (via [`Self::set_self_handle`]) right after the top-level `Arc::new` in `runtime.rs`;
    /// shared across `Clone`s because it is itself behind an `Arc`, so a clone tonic holds can
    /// still recover the original `Arc`. Used by the sandbox-IPC RPC bridge: a sandboxed session's
    /// `dial_and_bridge` builds a `DaemonRpcHandler` from `self_arc()` so the in-jail `tddy-tools`
    /// can reach the roster and conversation RPCs on this daemon over the `SessionChannel`.
    self_handle: Arc<std::sync::OnceLock<std::sync::Weak<ConnectionServiceImpl>>>,
}

mod seed_codebase;
pub use seed_codebase::*;

mod stack_parent;
pub use stack_parent::*;

mod seeded_clone_guard;
pub use seeded_clone_guard::*;

mod svc_resolve_tddy_tools_path;

mod svc_pr_status_for_caller;

mod svc_start_claude_cli_session;

mod hooks_and_urls;
pub use hooks_and_urls::*;

mod agent_roster;
pub(crate) use agent_roster::*;

#[allow(clippy::too_many_arguments)]
async fn spawn_claude_cli_session_inner(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    claude_cli_manager: &Arc<CliSessionManager>,
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
    dangerously_skip_permissions: bool,
    stack_parent: stack_parent::SpawnStackParent<'_>,
    managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>>,
    child_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ChildSpawnHandler>>,
    conversation_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>>,
    // When true, index the worktree into the session dir before launch (blocking; aborts the start
    // on failure) and point the `SemanticSearch` tool at that per-session index DB.
    semantic_index: bool,
    // When true (and the intent is new_branch_from_base), push the freshly created branch to origin
    // at session start; a push failure fails the start.
    create_remote_branch: bool,
    task_registry: &TaskRegistry,
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
    let projects_dir = projects_path_for_user(os_user, Some(tddy_data_dir))
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

    // Build branch intent and write a minimal changeset so the worktree setup fn can read it. A
    // legacy project (no stored default branch) leaves the base `None` so worktree setup resolves
    // the default live (`origin/master` → `origin/main` → `origin/HEAD`) — the same order the
    // project resolver uses.
    let ResolvedBranchWorkflow {
        intent,
        workflow: cs_workflow,
    } = resolve_branch_workflow(
        session_id,
        &BranchIntentRequest {
            branch_worktree_intent,
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
        },
        BranchIntentPolicy::claude_cli(),
        project.main_branch_ref.as_deref(),
    )?;
    let mut cs = Changeset {
        workflow: Some(cs_workflow),
        orchestrator_session_id: stack_parent.session_id().map(str::to_string),
        recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
        ..Changeset::default()
    };
    // A managed session seeds the recipe's start goal so `changeset.yaml` reflects the workflow
    // position immediately; the per-session controller advances it from there on `transition`.
    if let Some(recipe) = &managed_recipe {
        tddy_core::changeset::update_state(
            &mut cs,
            tddy_core::workflow::ids::WorkflowState::new(recipe.start_goal().as_str()),
        );
    }
    tddy_core::write_changeset(&session_dir, &cs)
        .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;

    let chain_base_ref = stack_parent
        .chain_base_ref(
            project_id,
            &sessions_base,
            &repo_root,
            new_branch_name,
            selected_integration_base_ref,
        )
        .await?;
    let worktree_base_ref =
        tddy_core::select_worktree_base_ref(selected_integration_base_ref, chain_base_ref);

    // Create the real git worktree (blocking: involves git fetch + git worktree add).
    let repo_root_clone = repo_root.clone();
    let session_dir_clone = session_dir.clone();
    let timeout = config.spawn_worker_request_timeout();
    let worktree_path = service_util::spawn_blocking_with_timeout(
        timeout,
        "start_claude_cli_session: create worktree",
        move || {
            tddy_core::setup_worktree_for_session_with_optional_chain_base(
                &repo_root_clone,
                &session_dir_clone,
                worktree_base_ref.as_deref(),
            )
            .map_err(|e| anyhow::anyhow!("worktree setup failed: {}", e))
        },
    )
    .await?;

    service_util::push_new_branch_to_origin_if_requested(
        create_remote_branch,
        intent,
        &session_dir,
        &worktree_path,
        timeout,
    )
    .await?;

    // The child's branch now exists (and, when requested, is on origin), so a pr-stack
    // orchestrator's planned node can record it — which is what lets this node's descendants be
    // spawned at all, since they base onto `<remote>/<branch>`. A spawn naming a node links that
    // node on whichever daemon owns the orchestrator; one naming none keeps the branch-derived
    // local write, so a session resuming the branch a node already owns re-links to that node.
    let remote =
        project_storage::effective_remote_name_for_project(&projects_dir, project_id, &repo_root)
            .map_err(|e| Status::internal(e.to_string()))?;
    // Resolved once: the same branch is what the node records and what this session publishes about
    // itself on its participant. Read back from the changeset the worktree setup just wrote rather
    // than taken from the request — the branch may carry a collision suffix, and a node recording a
    // name nobody created leaves every descendant basing onto a ref that does not exist.
    let spawned_branch = hooks_and_urls::spawned_branch_of_session(
        &session_dir,
        hooks_and_urls::effective_spawn_branch(
            branch_worktree_intent,
            new_branch_name,
            selected_branch_to_work_on,
            &remote,
        ),
    );
    // A link that fails does **not** fail the spawn (D36): it lands after the worktree, the branch
    // and the session already exist, so failing here would leave an orphan session on this host and
    // still no branch on the orchestrator's — strictly worse than a node the operator can re-link by
    // restarting it. The live association still travels in participant metadata (D37).
    stack_parent
        .link_spawned_branch_without_failing_the_spawn(&sessions_base, &spawned_branch, session_id)
        .await;

    let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
        config
            .claude_cli
            .as_ref()
            .and_then(|c| c.tddy_tools_path.as_deref()),
    );

    let daemon_url = hooks_and_urls::claude_hook_daemon_url(config);

    // Generate a per-session hook token and write .claude/settings.local.json into the
    // worktree. Claude Code reads this file on startup and wires the six lifecycle hooks.
    // Write failure is warn-and-continue so it never blocks the session from starting.
    let hook_token = Uuid::new_v4().to_string();
    hooks_and_urls::write_claude_hooks_settings(
        &worktree_path,
        &tddy_core::HookCommandParams {
            tddy_tools_path: &tddy_tools_path,
            daemon_url: &daemon_url,
            session_id,
            os_user,
            hook_token: &hook_token,
        },
    );

    // Spawn the claude CLI process in a PTY inside the real worktree. Resolve `claude` through the
    // shared host resolver — the same one the sandboxed path uses — so an explicit config path is
    // honored and a bare name is resolved to a real host install instead of relying on the daemon's
    // minimal systemd `PATH`.
    let manager = Arc::clone(claude_cli_manager);
    let session_id_owned = session_id.to_string();
    let model_owned = model.to_string();
    let binary_owned = hooks_and_urls::resolve_start_session_claude_binary(config);
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

    // Managed-workflow wiring: build the per-session controller + toolcall listener, write the
    // recipe's orchestration prompt to a file `claude` appends to its system prompt, and inject
    // a per-session TDDY_SOCKET (+ a PATH that resolves tddy-tools) so the agent's host-side
    // `tddy-tools transition` reaches this session's controller.
    let mut managed: Option<crate::session_toolcall::ManagedWorkflow> = None;
    let mut append_system_prompt_file: Option<PathBuf> = None;
    let mut env_extra: Vec<(String, String)> = Vec::new();
    if let Some(recipe) = managed_recipe.clone() {
        let launch = prepare_managed_workflow_inner(
            tddy_data_dir,
            session_id,
            recipe,
            &session_dir,
            &worktree_path,
            &session_dir,
            &tddy_tools_path,
            None,
            child_spawn_handler.clone(),
            conversation_spawn_handler.clone(),
        )?;
        append_system_prompt_file = Some(launch.prompt_file);
        env_extra = launch.env;
        managed = Some(launch.workflow);
    }

    // Semantic index: build the per-session vector index over the worktree before launching the
    // agent (blocking until terminal). A missing embedder or a failed index aborts the start — no
    // unindexed fallback. On success, point the `SemanticSearch` tool at the session's index DB.
    if semantic_index {
        let embedder = tddy_semantic_index::production_embedder(tddy_data_dir).map_err(|e| {
            Status::failed_precondition(format!(
                "semantic index requested but no embedder is available: {e}"
            ))
        })?;
        tddy_semantic_index::semantic_index::run_semantic_index_blocking(
            &worktree_path,
            &session_dir,
            embedder,
            task_registry,
            session_id,
        )
        .await
        .map_err(|e| Status::internal(format!("semantic index failed: {e}")))?;
        let (key, value) = tddy_semantic_index::semantic_index::semantic_index_env(&session_dir);
        env_extra.push((key, value));
    }

    let handle = manager
        .start_with_options(
            &session_id_owned,
            worktree_clone,
            &model_owned,
            &binary_owned,
            initial_prompt_opt.as_deref(),
            permission_mode_opt.as_deref(),
            dangerously_skip_permissions,
            false,
            append_system_prompt_file.as_deref(),
            Vec::new(),
            env_extra,
            Some(os_user),
        )
        .await
        .map_err(|e| Status::internal(format!("failed to spawn claude-cli: {}", e)))?;

    if let Some(mw) = managed {
        manager.attach_managed_workflow(session_id, mw).await;
    }

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
        cursor_chat_id: None,
        activity_status: None,
        hook_token: Some(hook_token),
        sandbox: None,
        agent: None,
        recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
    };
    tddy_core::write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {}", e)))?;

    // What this session tells the fleet about itself, recorded now and published when its terminal
    // is first bridged into LiveKit. The stack association is the load-bearing part: a PR-Stack view
    // on another host has no other way to learn that this session is the planned node's child
    // (D37), and it is knowledge this call has and a later reader does not — no session directory
    // records which planned node was materialized — so it is kept rather than re-derived.
    //
    // Recording it is local work over values already in hand. Putting a participant in the room is
    // not: it is a network round-trip to a server this daemon does not control, and a session is
    // made of a checkout and a process, both of which already exist by now. That join belongs to
    // the moment a LiveKit consumer arrives, which is the same moment the session's room is opened
    // — see `SessionRoomRegistry::ensure_open`. The desktop reaches its own host over IPC and
    // drives this terminal without a bridge at all.
    claude_cli_manager
        .expose_terminal_to_livekit(
            session_id,
            hooks_and_urls::claude_cli_participant_metadata(
                &hooks_and_urls::StartingClaudeCliSession {
                    session_id,
                    model,
                    recipe: managed_recipe
                        .as_ref()
                        .map(|r| r.name())
                        .unwrap_or_default(),
                    worktree_path: &worktree_path,
                    branch: &spawned_branch,
                    stack_parent: &stack_parent,
                },
            ),
        )
        .await;

    // Where that terminal will be served once it is bridged. Derived from the session id and the
    // deployment config rather than read off a connection, so it is the same answer whether a
    // consumer has arrived yet or not — and deriving it contacts nothing.
    let (lk_room, lk_url, lk_server_identity) = match spawner::livekit_creds_from_config(config) {
        Some(lk) => (
            spawner::resolve_livekit_room_name(lk.common_room.as_deref(), session_id),
            lk.url.clone(),
            spawner::livekit_server_identity_for_session(
                lk.daemon_instance_id.as_deref(),
                session_id,
            ),
        ),
        None => (String::new(), String::new(), String::new()),
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
        branch_conflict: None,
    }))
}

mod svc_resolve_listed_worktree;

mod terminal_bridge_impl;

mod svc_ensure_session_room_for_agents;

mod svc_provision_agent_clone;

mod svc_start_hosted_agent_clone;

mod svc_turn_end_reporter;

mod svc_start_sandboxed_claude_cli_session;

mod svc_start_sandboxed_cursor_cli_session;

mod svc_resume_claude_cli_session;

mod svc_split_context_from_codebase_host;

mod svc_relaunch_sandboxed_runner;

/// Launch inputs for a managed claude-cli session, produced by
/// [`ConnectionServiceImpl::prepare_managed_workflow`]: the workflow wiring (whose listener must be
/// kept alive for the session's lifetime), the orchestration-prompt file to append to claude's
/// system prompt, and the per-session env (`TDDY_SOCKET` + `PATH`) for host-side `tddy-tools`.
pub(crate) struct ManagedLaunch {
    workflow: crate::session_toolcall::ManagedWorkflow,
    prompt_file: PathBuf,
    env: Vec<(String, String)>,
}

/// Free-function form of [`ConnectionServiceImpl::prepare_managed_workflow`] so the shared
/// claude-cli spawn logic ([`spawn_claude_cli_session_inner`]) — which has no `self` — can reuse it.
/// `child_spawn_handler`, when present, is bound to the managed session's toolcall listener so the
/// agent's `pr_spawn_child` relay reaches a spawner (used for PR-stack orchestrators).
#[allow(clippy::too_many_arguments)]
fn prepare_managed_workflow_inner(
    tddy_data_dir: &Path,
    session_id: &str,
    recipe: Arc<dyn tddy_core::backend::WorkflowRecipe>,
    session_dir: &Path,
    worktree_path: &Path,
    prompt_dir: &Path,
    tddy_tools_path: &str,
    resume_at: Option<tddy_core::backend::GoalId>,
    child_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ChildSpawnHandler>>,
    conversation_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>>,
) -> Result<ManagedLaunch, Status> {
    let mw = match resume_at {
        Some(goal) => crate::session_toolcall::resume_managed_workflow(
            session_id,
            recipe,
            session_dir,
            worktree_path,
            tddy_data_dir,
            &std::env::temp_dir(),
            goal,
            child_spawn_handler,
            conversation_spawn_handler,
        ),
        None => crate::session_toolcall::set_up_managed_workflow(
            session_id,
            recipe,
            session_dir,
            worktree_path,
            tddy_data_dir,
            &std::env::temp_dir(),
            child_spawn_handler,
            conversation_spawn_handler,
        ),
    }
    .map_err(Status::internal)?;

    let prompt_path = prompt_dir.join("orchestration-prompt.txt");
    std::fs::write(&prompt_path, &mw.orchestration_prompt)
        .map_err(|e| Status::internal(format!("failed to write orchestration prompt: {e}")))?;

    // A managed session's `tddy-tools` MCP process needs these to locate the orchestrator's
    // changeset (`TDDY_SESSION_DIR`) and run `git` against the repo (`TDDY_REPO_DIR`) — the
    // PR-management tools read both. The tddy-coder TUI backends set them; the daemon's managed
    // claude-cli launch must set them here too, or those tools have no session/repo in scope.
    let mut env: Vec<(String, String)> = vec![
        (
            "TDDY_SOCKET".to_string(),
            mw.listener.socket_path().to_string_lossy().into_owned(),
        ),
        (
            "TDDY_SESSION_DIR".to_string(),
            session_dir.to_string_lossy().into_owned(),
        ),
        (
            "TDDY_REPO_DIR".to_string(),
            worktree_path.to_string_lossy().into_owned(),
        ),
    ];
    if let Some(dir) = std::path::Path::new(tddy_tools_path)
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
    {
        let existing = std::env::var("PATH").unwrap_or_default();
        env.push(("PATH".to_string(), format!("{}:{existing}", dir.display())));
    }
    Ok(ManagedLaunch {
        workflow: mw,
        prompt_file: prompt_path,
        env,
    })
}

/// Per-session [`ChildSpawnHandler`] for a PR-stack orchestrator: materializes a planned-PR node
/// into a child claude-cli session (with the orchestrator as `stack_parent`), reusing the same
/// [`spawn_claude_cli_session_inner`] the `StartSession` RPC uses. Bound only to a `pr-stack`
/// orchestrator's toolcall listener, so it can only spawn children for that orchestrator's stack.
struct StackChildSpawnHandler {
    /// Resolves each child's base off the orchestrator's stack. A collaborator rather than
    /// something built here because the orchestrator this handler spawns children of is a session
    /// of *this* daemon, so the resolution never leaves the host — but it takes the one path every
    /// spawn takes, rather than a second one that would drift from it.
    stack_parent_host: Arc<dyn StackParentHost>,

    /// The daemon whose attachment path materializes the child's documents. A shallow clone (every
    /// mutable field is behind an `Arc`), exactly as [`DaemonSeedCloneClaimant`] holds one: the
    /// documents go through [`ConnectionServiceImpl::prepare_session_attachments`], the same
    /// materializer `StartSession` uses, so a child cannot differ by how it was started.
    service: ConnectionServiceImpl,
    config: DaemonConfig,
    tddy_data_dir: PathBuf,
    claude_cli_manager: Arc<CliSessionManager>,
    os_user: String,
    project_id: String,
    sessions_base: PathBuf,
    orchestrator_session_id: String,
    orchestrator_session_dir: PathBuf,
}

mod child_spawn_handler;

/// The spawn *wiring*, both ways in: a child of a planned PR must come up holding the
/// orchestrator's documents and knowing to read its boundaries — whether the orchestrator agent
/// spawned it through `pr_spawn_child` or the operator started it from the Start-session dialog.
///
/// These drive real spawns. A git worktree is cut, the daemon's own attachment materializer runs,
/// and a stub standing in for `claude` records the command line it was handed — so deleting the
/// call to [`crate::stack_doc_attachments`] fails here, which no test of that pure helper does.
#[cfg(test)]
mod stack_child_spawn_tests;

/// Whether a managed session running `recipe_name` binds a `spawn-conversation` handler on its
/// toolcall listener. Only the grill-me recipe does — a plain TDD session has nothing to hand off,
/// and the PR-stack orchestrator uses `spawn-child` (resolving a planned node) instead.
pub(crate) fn recipe_enables_conversation_spawn(recipe_name: &str) -> bool {
    recipe_name == "grill-me"
}

/// Derive a git-friendly branch slug from a free-form conversation prompt when the agent did not
/// supply an explicit `branch`. Lowercased, non-alphanumeric runs collapsed to a single `-`, and
/// truncated so the worktree branch name stays reasonable. Falls back to a stable label when the
/// prompt has no usable characters.
fn conversation_branch_slug(prompt: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in prompt.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
        if slug.len() >= 40 {
            break;
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "spawned-conversation".to_string()
    } else {
        format!("conversation/{trimmed}")
    }
}

/// Per-session [`ConversationSpawnHandler`] for a managed session (grill-me): spawns a brand-new
/// interactive claude-cli conversation on a fresh worktree, tagged with the calling session as its
/// orchestrator, reusing the same [`spawn_claude_cli_session_inner`] the `StartSession` RPC uses.
/// The generic sibling of [`StackChildSpawnHandler`] — it takes a free-form prompt instead of
/// resolving a planned PR-stack node id, and the spawned conversation is itself unmanaged.
struct GrillMeConversationSpawnHandler {
    /// Resolves each spawned conversation's base off the orchestrator, which is a session of *this*
    /// daemon. Held for the same reason [`StackChildSpawnHandler::stack_parent_host`] is.
    stack_parent_host: Arc<dyn StackParentHost>,
    config: DaemonConfig,
    tddy_data_dir: PathBuf,
    claude_cli_manager: Arc<CliSessionManager>,
    os_user: String,
    project_id: String,
    sessions_base: PathBuf,
    orchestrator_session_id: String,
    orchestrator_session_dir: PathBuf,
    /// Fallback model when the orchestrator session's metadata has none (a tddy-coder *tool*
    /// session writes `model: None` to its metadata, unlike a claude-cli session). The daemon knows
    /// the model at spawn time and supplies it here so `spawn_conversation` can still inherit one.
    model_override: Option<String>,
}

mod conversation_spawn_handler;

/// Build the (agent name, replaced-tools) pairs a session's roster withdraws — one per attached
/// agent, each with its own `replaces`, normalized.
///
/// From the roster's snapshot of `replaces` rather than from the def each entry resolved from:
/// editing a YAML def or a registry assistant under a running session must not change what its main
/// agent may call (PRD § An entry).
///
/// The single source of what a session's roster withdraws: every spawn path — a fresh sandboxed
/// `claude-cli` session, a fresh sandboxed `cursor-cli` one, and a relaunch of either — computes the
/// withdrawal by calling this on the roster it is starting from, so there is one answer to derive
/// an appendix, an allowlist or a disallowlist from.
pub fn roster_replacement_pairs(
    agents: &[tddy_core::SessionAgentRecord],
) -> Vec<(String, Vec<String>)> {
    agents
        .iter()
        .map(|agent| {
            (
                agent.name.clone(),
                tddy_discovery::subagent::normalize_replaced_tools(&agent.replaces),
            )
        })
        .collect()
}

fn cleanup_materialized_attachments(session_dir: &Path, written: &[SessionAttachment]) {
    let attachments_dir = tddy_workflow::session_attachments_root(session_dir);
    for basename in written.iter().map(|a| &a.basename) {
        let path = attachments_dir.join(basename);
        if path.is_file() {
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!("cleanup_materialized_attachments: remove {path:?} failed: {e}");
            }
        }
    }
}

/// On-disk size of a just-materialized attachment. An unreadable entry reports 0 rather than
/// failing the start — the bytes are already written, and this value only feeds a progress event.
fn attachment_size_bytes(session_dir: &Path, basename: &str) -> u64 {
    std::fs::metadata(tddy_workflow::session_attachments_root(session_dir).join(basename))
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Where attachment-materialization progress goes while a start-session request is being served.
///
/// `StreamStartSession` supplies the stream's sender; unary `StartSession` supplies
/// [`AttachmentProgressSink::discarding`], so the two entry points run the identical code path and
/// the unary one simply has nowhere to report to.
pub(crate) struct AttachmentProgressSink {
    tx: Option<tokio::sync::mpsc::UnboundedSender<Result<StartSessionEvent, Status>>>,
}

impl AttachmentProgressSink {
    /// A sink that reports nowhere — the unary `StartSession` path.
    fn discarding() -> Self {
        Self { tx: None }
    }

    fn streaming(
        tx: tokio::sync::mpsc::UnboundedSender<Result<StartSessionEvent, Status>>,
    ) -> Self {
        Self { tx: Some(tx) }
    }

    /// Reports one attachment's progress. A closed receiver (the client hung up) is ignored: the
    /// session start is already under way and is not abandoned because nobody is watching.
    fn report(&self, progress: AttachmentMaterializationProgress) {
        let Some(tx) = self.tx.as_ref() else {
            return;
        };
        let _ = tx.send(Ok(StartSessionEvent {
            event: Some(StartSessionEventKind::AttachmentProgress(progress)),
        }));
    }
}

/// The attachment currently being materialized, bound to where its progress goes.
///
/// A source whose bytes arrive over time reports through this **as they arrive**, so a row's
/// progress bar moves during the transfer. That is not cosmetic: a forwarded stream terminates a
/// relay that goes [`crate::livekit_peer_discovery::PEER_FORWARD_STREAM_IDLE_TIMEOUT`] without a
/// frame, so reporting only once an attachment has fully landed would leave that per-frame deadline
/// covering a whole cross-host transfer.
pub(crate) struct AttachmentProgressReporter<'a> {
    sink: &'a AttachmentProgressSink,
    basename: &'a str,
    attachment_index: u32,
    attachment_count: u32,
}

impl AttachmentProgressReporter<'_> {
    fn report(&self, bytes_done: u64, bytes_total: u64) {
        self.sink.report(AttachmentMaterializationProgress {
            basename: self.basename.to_string(),
            attachment_index: self.attachment_index,
            attachment_count: self.attachment_count,
            bytes_done,
            bytes_total,
        });
    }
}

/// Everything materializing one start-session request's attachments needs: who asked, where the
/// session lives, what to attach, and where progress goes.
///
/// One cohesive context rather than six carried parameters — every field travels together from the
/// per-session-type branch in [`ConnectionServiceImpl::start_session_core`] down to the copy.
pub(crate) struct AttachmentMaterialization<'a> {
    session_token: &'a str,
    os_user: &'a str,
    sessions_base: &'a Path,
    session_id: &'a str,
    attachments: &'a [SessionAttachment],
    progress: &'a AttachmentProgressSink,
}

impl AttachmentMaterialization<'_> {
    fn session_dir(&self) -> PathBuf {
        self.sessions_base
            .join(SESSIONS_SUBDIR)
            .join(self.session_id)
    }
}

/// Where a session's git worktree lives relative to the daemon running its agent.
///
/// The second placement axis added by `docs/ft/daemon/remote-managed-worktree.md`:
/// `daemon_instance_id` still decides where the agent process runs, and this decides whose
/// filesystem holds the worktree it works in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodebasePlacement {
    /// Agent and worktree on the same daemon — every session created before split placement existed.
    CoLocated,
    /// Agent here, worktree on `codebase_instance_id`.
    Split { codebase_instance_id: String },
}

/// Classify a start request's codebase placement, refusing a split that cannot be honoured.
///
/// Mirrors [`crate::livekit_peer_discovery::classify_peer_route`]: a pure decision with every
/// precondition named in its error, so an operator learns *which* one failed rather than that the
/// request was bad. An empty or self-matching id is co-located — the pre-existing behaviour, which
/// this must never change.
///
/// A split needs `managed_codebase` (an agent that kept its native filesystem tools has nothing to
/// proxy through) and `session_type == "claude-cli"` (only Claude's `--allowedTools` /
/// `--disallowedTools` make the restriction enforceable rather than advisory — see the PRD
/// § Why claude-cli only), and the named daemon must be in the current eligible list.
pub fn classify_codebase_placement(
    local_instance_id: &str,
    requested_codebase_id: &str,
    eligible_ids: &[String],
    managed_codebase: bool,
    session_type: &str,
) -> Result<CodebasePlacement, String> {
    let requested = requested_codebase_id.trim();
    if requested.is_empty() || requested == local_instance_id.trim() {
        return Ok(CodebasePlacement::CoLocated);
    }
    if !managed_codebase {
        return Err(format!(
            "codebase_daemon_instance_id {requested:?} requires managed_codebase = true: an agent holding native filesystem tools has no reason to reach a worktree on another daemon"
        ));
    }
    let session_type = session_type.trim();
    if session_type != "claude-cli" {
        return Err(format!(
            "codebase_daemon_instance_id {requested:?} is only supported for session_type \"claude-cli\", not {session_type:?}: no other agent can be prevented from using its native filesystem tools"
        ));
    }
    if !eligible_ids.iter().any(|id| id.trim() == requested) {
        return Err(format!(
            "unknown or not connected codebase_daemon_instance_id {requested:?}: peer is not in the current eligible daemon list (configure livekit.common_room and ensure the peer is in the same LiveKit room)"
        ));
    }
    log::info!("classify_codebase_placement: codebase placed on peer instance_id={requested}");
    Ok(CodebasePlacement::Split {
        codebase_instance_id: requested.to_string(),
    })
}

/// Whether a peer's `DeleteSession` failure says the session is not there, as opposed to saying
/// nothing usable about it.
///
/// `session_deletion::delete_session_directory` answers `failed_precondition` for a session id it
/// holds no directory for — it reads as wrong-daemon routing there — and `not_found` is the same
/// answer from any layer that phrases it that way. Both mean the worktree is provably gone with the
/// session that owned it. Every other code, and every transport failure, leaves that unknown, which
/// is a different thing and must not be treated as success.
/// What a split start failed with, as far as the teardown that unwinds it is concerned.
///
/// The distinction is not cosmetic: it decides whether the codebase daemon answering "I have no
/// such session" proves the session was never created, or only that it did not exist yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitStartFailure {
    /// A verdict was reached within the deadline — the peer refused, answered something
    /// unusable, or the agent spawn on this host failed. Whatever the peer holds now is final.
    PeerAnswered,
    /// The forwarded start ran out of time. The peer is still free to finish the work it was
    /// doing, so nothing about its current state is final.
    ForwardDeadline,
}

impl SplitStartFailure {
    /// Classify the error a forwarded start came back with. Only [`Code::DeadlineExceeded`] leaves
    /// the peer still working — every other status means it answered.
    fn from_forward_error(status: &Status) -> Self {
        if status.code == tddy_rpc::Code::DeadlineExceeded {
            Self::ForwardDeadline
        } else {
            Self::PeerAnswered
        }
    }
}

fn peer_has_no_such_session(status: &Status) -> bool {
    matches!(
        status.code,
        tddy_rpc::Code::FailedPrecondition | tddy_rpc::Code::NotFound
    )
}

/// Validate `StartSessionRequest.split_agent`: the agent session, on another daemon, that a
/// `workspace` session is being created to hold the worktree for.
///
/// Only `workspace` sessions accept one, for the same reason `requested_session_id` is restricted
/// that way — it records a fact about a checkout, and no other session type has one to record. A
/// caller sending it with any other type is refused rather than having it dropped: the field is what
/// makes a withdrawal enforceable on that session, so silently ignoring it would accept a pairing
/// that never got written and refuse the operator's agents later, on a session that looks paired
/// from the caller's side.
///
/// An agent clone is refused the same way. Both fields describe what a `workspace` checkout is
/// *for*, and a checkout cannot be both a clone's mirror and a split session's working tree.
///
/// Both halves of the placement are required. A daemon named with no session on it names a host but
/// nothing that works in the checkout — see [`tddy_core::paired_agent`], which reads back
/// what this writes and applies the same rule.
fn resolve_split_agent_placement(
    split_agent: Option<&SplitAgentPlacement>,
    session_type: &str,
    is_agent_clone: bool,
) -> Result<Option<workspace_session::PairedAgentSession>, Status> {
    let Some(placement) = split_agent else {
        return Ok(None);
    };
    let session_type = session_type.trim();
    if session_type != "workspace" {
        return Err(Status::invalid_argument(format!(
            "split_agent is only supported for session_type \"workspace\", not {session_type:?}"
        )));
    }
    if is_agent_clone {
        return Err(Status::invalid_argument(
            "split_agent and agent_clone describe what a workspace checkout is for, and a checkout \
             cannot be both an agent clone's mirror and a split session's working tree",
        ));
    }
    let agent_session_id = placement.session_id.trim();
    let agent_daemon_instance_id = placement.agent_daemon_instance_id.trim();
    if agent_session_id.is_empty() || agent_daemon_instance_id.is_empty() {
        return Err(Status::invalid_argument(format!(
            "split_agent placement is incomplete: session_id and agent_daemon_instance_id are both \
             required to record which agent works in this workspace's worktree (got \
             session_id={agent_session_id:?}, agent_daemon_instance_id={agent_daemon_instance_id:?})"
        )));
    }
    Ok(Some(workspace_session::PairedAgentSession {
        daemon_instance_id: agent_daemon_instance_id.to_string(),
        session_id: agent_session_id.to_string(),
    }))
}

/// Validate `StartSessionRequest.requested_session_id`: the id a caller asks the session to be
/// created under instead of one this daemon generates.
///
/// Only `workspace` sessions accept one, and only because of atomicity: the daemon placing a split
/// session's worktree here has to know the id *before* it forwards the start, so that a forward
/// which errors or times out can still name the session to tear down (see
/// `docs/ft/daemon/remote-managed-worktree.md` § Failure is atomic). Every other session type
/// refuses it rather than ignoring it — a caller that believed it had pinned the id would go on to
/// address a session that does not exist.
///
/// The id becomes a directory name under the sessions base, so it is validated exactly as the id
/// `DeleteSession` is handed.
pub fn resolve_caller_chosen_session_id(
    requested_session_id: &str,
    session_type: &str,
) -> Result<Option<String>, Status> {
    let requested = requested_session_id.trim();
    if requested.is_empty() {
        return Ok(None);
    }
    let session_type = session_type.trim();
    if session_type != "workspace" {
        return Err(Status::invalid_argument(format!(
            "requested_session_id is only supported for session_type \"workspace\", not {session_type:?}"
        )));
    }
    validate_session_id_segment(requested).map_err(|e| {
        Status::invalid_argument(format!("invalid requested_session_id: {}", e.message()))
    })?;
    Ok(Some(requested.to_string()))
}

mod svc_resolve_os_user;

mod svc_materialize_staged_attachment;

mod svc_spawn_split_agent;

mod svc_start_session_core;

mod svc_terminal_ports;

mod svc_session_files_ports;

/// The daemon's half of `activity.ActivityService` — the six host answers families M and N read,
/// and the routing the daemon keeps. `#unbundle` node 7.
mod svc_activity_ports;

mod family_proto_bridge;
mod svc_catalog_ports;
mod svc_exec_tool_ports;
mod svc_family_entries;
mod svc_pr_stack_ports;
/// The daemon's half of `session_agents.SessionAgentService` — the host capabilities family B
/// reads, and the routing the daemon keeps. `#unbundle` node 7.
mod svc_session_agent_ports;

pub use svc_session_files_ports::PeerRoutedSessionFiles;

/// Node 7's two served surfaces, named because the local Unix socket mounts them: the bundle
/// `local_socket_server` takes is generic over the implementation each generated adapter wraps, so
/// the host that assembles it has to be able to write these two types down.
pub use svc_activity_ports::PeerRoutedActivity;
pub use svc_session_agent_ports::PeerRoutedSessionAgents;

/// Merge local `ListProjects` rows with [`EligibleDaemonSource::peer_project_entries`].
async fn merge_listed_projects_with_peers(
    eligible: &dyn EligibleDaemonSource,
    session_token: &str,
    local: Vec<ProtoProjectEntry>,
) -> Vec<ProtoProjectEntry> {
    let peer_rows = eligible.peer_project_entries(session_token).await;
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

/// The host-side RPC dispatch for a sandboxed session's `SessionChannel`: routes the roster and
/// conversation RPCs the in-jail `tddy-tools` issues (forwarded by the runner as `RpcRequest`s)
/// to this daemon's `ConnectionServiceImpl`. The runner's `ToolExecService` forwards
/// `StreamSessionAgents` / `OpenAgentConversation` / `PromptAgentConversation` /
/// `CancelAgentConversation` / `ReportAgentConversationState`; this handler decodes each, calls the
/// matching typed method on the
/// `Arc<ConnectionServiceImpl>` it holds, and returns the encoded response — unary for the two
/// unary RPCs, a server stream of encoded frames for the two streaming ones. `tonic::Status`
/// errors are carried back to the in-jail caller as a single terminal `RpcStreamFrame` with
/// `error` set, which the runner's relay turns into the `tddy_rpc::Status` the caller sees.
struct DaemonRpcHandler {
    conn: Arc<ConnectionServiceImpl>,
}

mod daemon_rpc_handler;

mod rpc_service;

/// Reject an obvious path traversal in a path-bearing exec tool's arguments, before any I/O.
///
/// The worktree root is the boundary an exec tool call is confined to; a `..` component asks to
/// leave it, which is refused rather than normalized away.
fn reject_exec_tool_path_traversal(tool_name: &str, args_json: &str) -> Result<(), Status> {
    if !matches!(tool_name, "Read" | "Write" | "StrReplace" | "Delete") {
        return Ok(());
    }
    let args: serde_json::Value =
        serde_json::from_str(args_json).unwrap_or(serde_json::Value::Null);
    let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
        return Ok(());
    };
    if Path::new(path)
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return Err(Status::permission_denied(
            "path contains '..' components (traversal rejected)",
        ));
    }
    Ok(())
}

/// Bytes of tool result carried per `StreamExecuteTool` frame.
///
/// Defined *as* [`HOST_DOCUMENT_FRAME_BYTES`] rather than as the same number, because the budget is
/// a property of the transport rather than of what rides on it: both are what every transport in the
/// stack carries per message without applying its own chunk framing. Two constants free to drift
/// would be two answers to one question, and only one of them could be right.
pub const EXEC_TOOL_FRAME_BYTES: usize = HOST_DOCUMENT_FRAME_BYTES;

/// Split a completed tool result into ordered [`EXEC_TOOL_FRAME_BYTES`] frames.
///
/// The outcome rides the **final** frame — a tool error is a result, not an RPC failure, matching
/// unary `ExecuteTool`'s contract. An empty result still yields exactly one frame, so a consumer
/// never has to tell "empty result" from "stream produced nothing", and a stream ending without a
/// `last` frame is unambiguously a truncation.
fn exec_tool_result_frames(response: ExecuteToolResponse) -> Vec<ExecuteToolChunk> {
    let bytes = response.result_json.into_bytes();
    let mut frames: Vec<ExecuteToolChunk> = bytes
        .chunks(EXEC_TOOL_FRAME_BYTES)
        .map(|chunk| ExecuteToolChunk {
            result_chunk: chunk.to_vec(),
            ..Default::default()
        })
        .collect();
    if frames.is_empty() {
        frames.push(ExecuteToolChunk::default());
    }
    let last = frames.last_mut().expect("at least one frame");
    last.is_error = response.is_error;
    last.error_message = response.error_message;
    last.job_id = response.job_id;
    last.job_running = response.job_running;
    last.last = true;
    frames
}

/// Bytes to leave free in a LiveKit data packet for everything in a frame that is not payload: the
/// RPC envelope (request id, service/method metadata, sender identity) plus the frame's own fields —
/// `total_byte_size` for a document chunk, `error_message` / `job_id` / the flags for a tool-result
/// chunk. Mirrors the web's `UPLOAD_REQUEST_ENVELOPE_HEADROOM`, which sizes the same budget from the
/// other end of the same transport.
const FRAME_ENVELOPE_HEADROOM: usize = 8 * 1024;

/// A frame plus its envelope must fit in one LiveKit data packet. Past that budget the transport
/// splits each frame into chunk frames, and one lost chunk frame leaves the peer's reassembler
/// permanently incomplete — the call is then never answered and never fails
/// (`docs/ft/coder/rpc-multi-transport.md`). A build failure here is the point: the doc comment above
/// asserts "without its own chunk framing", and raising the frame size to 64 KiB would silently make
/// that false. One assert covers [`EXEC_TOOL_FRAME_BYTES`] too, which is this same constant.
///
/// [`FRAME_ENVELOPE_HEADROOM`] is a *shared* budget, not a per-field one, and one frame type spends
/// more of it than the rest: `ContextFileBatchChunk` repeats `rel_path` on every frame, so a deeply
/// nested `.claude/skills/…/SKILL.md` eats path-length bytes out of the same 8 KiB the RPC envelope
/// uses. 8 KiB against a path bounded by `PATH_MAX` leaves that comfortable today — this is a note
/// for whoever narrows the headroom or adds a frame field, not a live risk.
const _: () = assert!(
    HOST_DOCUMENT_FRAME_BYTES + FRAME_ENVELOPE_HEADROOM
        <= tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES,
    "HOST_DOCUMENT_FRAME_BYTES must fit in one LiveKit data packet with envelope headroom"
);

/// Split one [`ActivityDelta`]'s patch into ordered [`HOST_DOCUMENT_FRAME_BYTES`] frames.
///
/// Every frame carries the whole description — `seq`, `prev_seq`, `base_commit`,
/// `total_byte_size` and `scoped_paths` — for the reason the wire contract gives: a reader knows
/// what it is receiving from the first frame, and a client can check the server scoped the way it
/// asked rather than trusting that it did.
///
/// A call that changed nothing is **one** frame with an empty patch and `total_byte_size` 0 — AC9.
/// That is the same discipline [`worktree_file_frames`] applies, and for the same reason: an empty
/// answer must not look like a failed one.
pub fn activity_delta_frames(delta: &ActivityDelta) -> Vec<AgentActivityDeltaChunk> {
    let total_byte_size = delta.patch.len() as u64;
    let describe = |patch: Vec<u8>| AgentActivityDeltaChunk {
        patch,
        seq: delta.seq,
        prev_seq: delta.prev_seq,
        base_commit: delta.base_commit.clone(),
        total_byte_size,
        scoped_paths: delta.scoped_paths.clone(),
    };
    let mut frames: Vec<AgentActivityDeltaChunk> = delta
        .patch
        .chunks(HOST_DOCUMENT_FRAME_BYTES)
        .map(|chunk| describe(chunk.to_vec()))
        .collect();
    if frames.is_empty() {
        frames.push(describe(Vec::new()));
    }
    frames
}

/// Guard for any RPC that mutates a `"pr-stack"` orchestrator's `Changeset.stack`: rejects a
/// session whose recipe (or legacy alias) doesn't resolve to `"pr-stack"`, before the caller
/// touches that session's changeset. Shared by `add_planned_pr` today; future planned-PR
/// mutation RPCs (edit/delete) should call this too rather than re-checking inline.
fn require_pr_stack_orchestrator(session_dir: &std::path::Path) -> Result<(), Status> {
    let changeset = tddy_core::read_changeset(session_dir)
        .map_err(|e| Status::invalid_argument(e.to_string()))?;
    let recipe_name = changeset.recipe.as_deref().unwrap_or("");
    let is_pr_stack =
        tddy_workflow_recipes::recipe_resolve::resolve_workflow_recipe_from_cli_name(recipe_name)
            .map(|r| r.name() == "pr-stack")
            .unwrap_or(false);
    if !is_pr_stack {
        return Err(Status::failed_precondition(
            "session is not a pr-stack orchestrator",
        ));
    }
    Ok(())
}

/// Refuse a `StartSessionRequest.pr_stack_base_session_id` that cannot seed a stack, *before*
/// anything spawns.
///
/// The pre-spawn position is the point: a refusal raised after the spawn is invisible to the
/// new-session form, which has already navigated away, so the operator would be left with an
/// orchestrator that looks seeded and is not. The seeding function refuses the same conditions again
/// as its own writer contract — the CLI flag is reachable without this RPC.
///
/// A blank id validates nothing, because nothing was asked for: an unseeded orchestrator is the
/// pre-existing behaviour. Otherwise the recipe must resolve to `"pr-stack"` (the legacy
/// `plan-pr-stack` / `orchestrate-pr-stack` aliases resolve to it and are accepted — refusing them
/// would make the recipe name a load-bearing string rather than a resolution), the named session must
/// pass [`tddy_workflow_recipes::pr_stack::check_stack_seed_base`] — the *same* rules, in the *same*
/// words, that the seeding writer enforces — and its repository must be the requesting project's.
///
/// **The repository check is this function's own**, because only the RPC knows which project was
/// asked for. Without it an operator can seed a stack with a branch from a different repository:
/// nothing refuses it, and the failure lands much later as a git error when the first descendant tries
/// to base off `origin/<branch>`, by which time the orchestrator exists and looks seeded. It compares
/// **canonicalized repository roots**, never project ids — a project id is registry-local and not
/// stable across hosts, while the repository root is the thing a stacked branch must actually share.
// `result_large_err`: the refusal is what the tonic gRPC surface reports to the new-session form, so
// `tonic::Status` is the error type — the same reason the adapter's streaming handlers allow it.
#[allow(clippy::result_large_err)]
pub fn validate_stack_seed_base_session(
    sessions_base: &Path,
    recipe: &str,
    base_session_id: &str,
    project_repo_root: &Path,
) -> Result<(), tonic::Status> {
    let base_session_id = base_session_id.trim();
    if base_session_id.is_empty() {
        return Ok(());
    }

    let is_pr_stack =
        tddy_workflow_recipes::recipe_resolve::resolve_workflow_recipe_from_cli_name(recipe.trim())
            .map(|r| r.name() == "pr-stack")
            .unwrap_or(false);
    if !is_pr_stack {
        return Err(tonic::Status::invalid_argument(format!(
            "pr_stack_base_session_id is only supported for the pr-stack recipe, but this session \
             requested recipe {recipe:?}"
        )));
    }

    // One rule, one wording: the refusal text lives in the recipes crate beside the writer, and only
    // the *code* it travels as is decided here — an id that names nothing is a bad argument, a session
    // whose state cannot seed is a failed precondition.
    let base =
        tddy_workflow_recipes::pr_stack::check_stack_seed_base(sessions_base, base_session_id)
            .map_err(|refusal| match refusal {
                tddy_workflow_recipes::pr_stack::StackSeedBaseRefusal::Unresolvable(reason) => {
                    tonic::Status::invalid_argument(reason)
                }
                tddy_workflow_recipes::pr_stack::StackSeedBaseRefusal::Unusable(reason) => {
                    tonic::Status::failed_precondition(reason)
                }
            })?;

    let base_repo = base.repo_path.as_deref().ok_or_else(|| {
        tonic::Status::failed_precondition(format!(
            "session '{base_session_id}' records no repository, so it cannot be confirmed to work in \
             this project's repository"
        ))
    })?;
    if !session_repo_is_in_project(Path::new(base_repo), project_repo_root).map_err(|reason| {
        tonic::Status::failed_precondition(format!(
            "session '{base_session_id}' could not be checked against this project's repository: \
             {reason}"
        ))
    })? {
        return Err(tonic::Status::failed_precondition(format!(
            "session '{base_session_id}' works in repository '{base_repo}', not this project's \
             '{}', so its branch cannot be stacked on here",
            project_repo_root.display()
        )));
    }
    Ok(())
}

/// Whether a session's recorded repository is the project's repository, or a worktree inside it.
///
/// The relation is "at or under", not equality, because `Changeset.repo_path` records the project's
/// main repo for a `tddy-coder` session but the session's **own worktree**
/// (`<repo>/.worktrees/<name>`) for a claude-cli / cursor-cli / workspace session. Both work in the
/// project's repository; only one of them spells its root.
///
/// Both sides are canonicalized: a project registered through a symlinked path and a session that
/// recorded the resolved one name the same repository, and a string comparison would call them
/// different. An unresolvable path is an `Err`, not a `false` — "could not tell" and "different
/// repository" are different answers, and only one of them may be reported as a mismatch.
fn session_repo_is_in_project(
    session_repo: &Path,
    project_repo_root: &Path,
) -> Result<bool, String> {
    let canonical = |path: &Path| -> Result<std::path::PathBuf, String> {
        path.canonicalize()
            .map_err(|e| format!("'{}' could not be resolved: {e}", path.display()))
    };
    Ok(canonical(session_repo)?.starts_with(canonical(project_repo_root)?))
}

/// Derive `owner/repo` from a repo's `origin` remote URL, for GitHub API namespacing.
/// Returns `None` when the remote can't be read or isn't a recognizable GitHub URL.
fn owner_repo_from_repo_root(repo_root: &std::path::Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let remote_url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    tddy_workflow_recipes::orchestrate_pr_stack::github::owner_repo_from_remote_url(&remote_url)
}

/// A PR status the daemon could not look up: *unavailable* with an operator-facing `reason`, never
/// `exists = false` (D8). Logged, because a lookup that never happened is otherwise invisible — the
/// daemon log carried no PR line at all for an orchestrator polled hundreds of times.
fn pr_status_unavailable(
    branch: &str,
    reason: String,
) -> tddy_service::proto::connection::PrStatusView {
    log::warn!("PR status unavailable for branch {branch}: {reason}");
    tddy_service::proto::connection::PrStatusView {
        unavailable: true,
        unavailable_reason: reason,
        ..Default::default()
    }
}

/// Compare `branch` against `base_branch`, reading through the process-wide cache.
///
/// Resolving the two refs is a pair of `rev-parse`s and runs every time — it is what produces the
/// cache key, and it is also how a moved ref is noticed. Only the comparison itself, which runs
/// `git merge-tree`, is cached.
fn base_sync_through_cache(
    repo_root: &std::path::Path,
    branch: &str,
    base_branch: &str,
) -> Result<tddy_core::base_sync::BranchBaseSync, String> {
    let refs = tddy_core::base_sync::resolve_base_sync_refs(repo_root, branch, base_branch)?;
    let key = crate::base_sync_cache::BaseSyncKey::new(repo_root, &refs);
    crate::base_sync_cache::shared().get_or_probe(key, || {
        tddy_core::base_sync::compare_base_sync_refs(repo_root, &refs)
    })
}

/// A completed comparison on the wire. `base_branch` carries the ref that was actually compared —
/// not the one the caller asked for — because the counts are meaningless beside a ref they did not
/// come from (D28).
fn base_sync_view(
    sync: tddy_core::base_sync::BranchBaseSync,
) -> tddy_service::proto::connection::BranchBaseSync {
    tddy_service::proto::connection::BranchBaseSync {
        base_branch: sync.base_ref.clone(),
        behind_count: sync.behind_count,
        ahead_count: sync.ahead_count,
        has_conflicts: sync.has_conflicts,
        conflicted_paths: sync.conflicted_paths,
        unavailable: false,
        unavailable_reason: String::new(),
        base_ref: sync.base_ref,
        head_ref: sync.head_ref,
    }
}

/// A comparison the daemon could not make: *unavailable* with an operator-facing reason, never a
/// zeroed success. A failed comparison reads identically to a healthy one on every other field, so
/// this discriminator is the only thing standing between "could not tell" and "clean" (D27).
fn base_sync_unavailable(
    base_branch: &str,
    reason: &str,
) -> tddy_service::proto::connection::BranchBaseSync {
    tddy_service::proto::connection::BranchBaseSync {
        base_branch: base_branch.to_string(),
        unavailable: true,
        unavailable_reason: reason.to_string(),
        ..Default::default()
    }
}

/// The `worktree` leg of a `BranchResolution`: the on-disk worktree checked out for `branch`, and
/// whether it holds outstanding work.
///
/// Two git subprocesses — a `git worktree list` walk and a `git status --porcelain` — so every caller
/// runs this on the blocking pool, never on a runtime thread.
fn worktree_leg(
    repo_root: Option<&std::path::Path>,
    branch: &str,
) -> tddy_service::proto::connection::BranchWorktree {
    use tddy_service::proto::connection::BranchWorktree;

    let Some(path) =
        repo_root.and_then(|root| tddy_core::worktree::worktree_path_for_branch(root, branch))
    else {
        return BranchWorktree::default();
    };
    let dirty_paths = worktree_dirty_paths(&path);
    BranchWorktree {
        exists: true,
        path: path.to_string_lossy().into_owned(),
        dirty: !dirty_paths.is_empty(),
        dirty_paths,
    }
}

/// The tracked paths with outstanding changes in a worktree — empty for a clean one, and empty for
/// a path git cannot read at all, which is the same thing as far as offering a pull goes.
///
/// Untracked files are deliberately excluded: git refuses loudly rather than clobbering one, and
/// counting them would leave the pull control permanently blocked in any worktree an agent works in.
fn worktree_dirty_paths(worktree: &std::path::Path) -> Vec<String> {
    tddy_workflow_recipes::orchestrate_pr_stack::worktree_is_clean(worktree).unwrap_or_else(|e| {
        log::warn!(
            "QueryBranch: could not read the state of the worktree at {}: {e}",
            worktree.display()
        );
        Vec::new()
    })
}

/// GitHub PR state → the lowercase label carried on the `PrStatusView.state` wire field.
fn pr_state_label(
    state: tddy_workflow_recipes::orchestrate_pr_stack::github::PrState,
) -> &'static str {
    use tddy_workflow_recipes::orchestrate_pr_stack::github::PrState;
    match state {
        PrState::Open => "open",
        PrState::Merged => "merged",
        PrState::Closed => "closed",
        PrState::Draft => "draft",
    }
}

/// TTL for the per-(agent, daemon) model-probe cache. A probe spawns a subprocess and may hit the
/// network, so results are cached briefly to avoid re-probing on every agent toggle in the UI.
const AGENT_MODELS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(60);

#[allow(clippy::type_complexity)]
static AGENT_MODELS_CACHE: std::sync::OnceLock<
    std::sync::Mutex<
        std::collections::HashMap<String, (std::time::Instant, ListAgentModelsResponse)>,
    >,
> = std::sync::OnceLock::new();

fn agent_models_cache() -> &'static std::sync::Mutex<
    std::collections::HashMap<String, (std::time::Instant, ListAgentModelsResponse)>,
> {
    AGENT_MODELS_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Build the `tddy-tools list-models` argv for an agent probe. Always `["list-models", "--agent",
/// <agent>]`; appends `["--cursor-cli-path", <path>]` only when probing `cursor` with a resolved
/// path, so the impersonated child execs the fully-qualified binary instead of a PATH lookup.
fn list_models_probe_args(agent: &str, cursor_cli_path: Option<&std::path::Path>) -> Vec<String> {
    let mut args = vec![
        "list-models".to_string(),
        "--agent".to_string(),
        agent.to_string(),
    ];
    if agent == "cursor" {
        if let Some(path) = cursor_cli_path {
            args.push("--cursor-cli-path".to_string());
            args.push(path.to_string_lossy().into_owned());
        }
    }
    args
}

/// Parse the JSON stdout of `tddy-tools list-models --agent <id>`
/// (`{"models":[{"id":..,"label":..}],"default_model":".."}`) into a `ListAgentModelsResponse`.
/// Malformed output is a hard error — a failed probe must not look like an empty catalog.
fn parse_agent_models_json(stdout: &str) -> Result<ListAgentModelsResponse, Status> {
    #[derive(serde::Deserialize)]
    struct ModelJson {
        id: String,
        label: String,
    }
    #[derive(serde::Deserialize)]
    struct ModelsJson {
        models: Vec<ModelJson>,
        default_model: String,
    }
    let parsed: ModelsJson = serde_json::from_str(stdout.trim())
        .map_err(|e| Status::internal(format!("failed to parse list-models output: {e}")))?;
    Ok(ListAgentModelsResponse {
        models: parsed
            .models
            .into_iter()
            .map(|m| CatalogModelInfo {
                id: m.id,
                label: m.label,
            })
            .collect(),
        default_model: parsed.default_model,
    })
}

#[cfg(test)]
mod signal_session_unit_tests;

#[cfg(test)]
mod delete_session_unit_tests;

#[cfg(test)]
mod list_sessions_unit_tests;

#[cfg(test)]
mod report_session_status_unit_tests;

#[cfg(test)]
mod agent_activity_unit_tests;

/// Resuming a session must relaunch its child with the *same* coding agent and workflow recipe it
/// was originally started with — read back from the persisted `.session.yaml`. Before this,
/// `ResumeSession` hard-coded `agent: None` / `recipe: None`, so tddy-coder fell back to its
/// default agent (`claude`), turning a resumed `cursor` / `pr-stack` session into a broken
/// `claude --resume <foreign-id>` run.
#[cfg(test)]
mod resume_agent_recipe_restore_tests;

#[cfg(test)]
mod specialized_subagent_env_unit_tests;

#[cfg(test)]
mod seeded_roster_records_unit_tests;

#[cfg(test)]
mod add_planned_pr_unit_tests;

/// A spawned child must record its **branch** on the planned node it materializes. Without that
/// forward link the orchestrator's stack still reads "no branch anywhere", so `base_ref_for_spawn`
/// refuses every descendant — a stack wedged at its bottom node. The child session id is recorded
/// alongside it only as a fallback route back to the branch.
#[cfg(test)]
mod stack_child_link_tests;

#[cfg(test)]
mod cross_daemon_session_token_acceptance_tests;

#[cfg(test)]
mod list_agent_models_parse_tests;

#[cfg(test)]
mod start_session_binary_resolution_tests;

#[cfg(test)]
mod resume_session_binary_resolution_tests;

/// Where a session's worktree comes from. A local client (e.g. tddy-sandbox-app) may send an explicit
/// `repo_path` to use directly; otherwise the worktree is resolved from a registered `project_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeSource {
    /// Use this local checkout path directly (client-supplied).
    RepoPath(std::path::PathBuf),
    /// Resolve from the registered project id.
    Project(String),
}

/// Pure: choose the worktree source for a session from the request's `repo_path` / `project_id`.
/// A non-empty `repo_path` wins (local-client path); otherwise fall back to `project_id`.
pub fn session_worktree_source(repo_path: &str, project_id: &str) -> WorktreeSource {
    if repo_path.is_empty() {
        WorktreeSource::Project(project_id.to_string())
    } else {
        WorktreeSource::RepoPath(std::path::PathBuf::from(repo_path))
    }
}

/// Pure: assemble the pass-through argument tokens forwarded to the in-jail `claude` for a
/// sandboxed session, in the order `claude` must receive them.
///
/// Client-supplied `claude_args` come first, verbatim (e.g. `--add-dir /foo`). A non-empty
/// `initial_prompt` is appended last as a trailing positional, so it lands as the first user turn
/// even when extra flags precede it; an empty/whitespace prompt is omitted. The runner wraps each
/// returned token in a `--claude-arg` occurrence and inserts them after `claude`'s fixed flags and
/// before the MCP allowlist args (see `SpawnClaudePtyParams::claude_args`), which keeps a trailing
/// positional a positional instead of being swallowed by the variadic `--mcp-config`.
pub fn sandbox_claude_passthrough_args(
    claude_args: &[String],
    initial_prompt: &str,
) -> Vec<String> {
    let mut out: Vec<String> = claude_args.to_vec();
    let prompt = initial_prompt.trim();
    if !prompt.is_empty() {
        out.push(prompt.to_string());
    }
    out
}

#[cfg(test)]
mod worktree_source_tests;

#[cfg(test)]
mod sandbox_claude_passthrough_args_tests;

#[cfg(test)]
mod conversation_spawn_wiring_tests;

#[cfg(test)]
mod list_agent_models_probe_tests;

#[cfg(test)]
mod remote_branch_push_gating_tests;

#[cfg(test)]
mod workspace_start_request_unit_tests;

/// A seeded roster agent on a sandboxed workspace session reaches files through the **same** jail
/// the remote `ExecuteTool` callers do.
///
/// PRD: `docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox.
/// Changeset: `docs/dev/changesets/`, 2026-08-30 workspace tool sandbox.
///
/// Lives here rather than in `tests/workspace_tool_sandbox_acceptance.rs` because
/// [`ConnectionServiceImpl::local_agent_codebase_access`] is the seam under test and it is private:
/// widening it to `pub` purely so a test could call it would export an internal for no other
/// caller. This is the roster half of the dispatch contract; the remote-caller half is proven from
/// the outside, over the RPC surface.
#[cfg(test)]
mod workspace_sandbox_roster_dispatch_unit_tests;
