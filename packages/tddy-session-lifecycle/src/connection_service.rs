//! ConnectionService implementation for daemon session/tool management.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::cli_session_manager::CliSessionManager;
use crate::config::DaemonConfig;
use tddy_daemon_livekit::livekit_rooms_stream::RoomRoster;
use tddy_daemon_livekit::session_room::ActivityDelta;
use tddy_service::proto::activity::AgentActivityDeltaChunk;
use tddy_spawn::spawn_worker;
use tddy_task::TaskRegistry;

// Bound for the extracted test modules, which reach the code under test through `use super::*`.
// The lib itself no longer names any of these — every user moved into `connection_service/` — so a
// plain `use` would be an unused import there. `#[cfg(test)]` keeps them out of the lib build
// entirely rather than trading a resolution error for a lint.
#[cfg(test)]
#[cfg(test)]
use crate::livekit_peer_discovery::local_instance_id_for_config;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
#[cfg(test)]
#[cfg(test)]
use tddy_core::session_lifecycle::unified_session_dir_path;
#[cfg(test)]
use tddy_core::Changeset;
#[cfg(test)]
use tddy_rpc::Request;
#[cfg(test)]
use tddy_service::proto::exec_tools::ExecuteToolRequest;
#[cfg(test)]
use tddy_service::proto::session::SessionService as SessionServiceTrait;
#[cfg(test)]
use tddy_service::proto::session::{SessionAttachment, SplitAgentPlacement};
#[cfg(test)]
use tddy_service::proto::session::{Signal, SignalSessionRequest, StartSessionRequest};

use tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES;

mod service_util;
pub(crate) use service_util::*;
/// The deadlines every clone and supervised spawn runs under — shared with the project handlers in
/// `tddy-daemon-rpc`, which clone repositories the way session starts do.
pub use service_util::{await_supervised_with_timeout, spawn_blocking_with_timeout};

/// Stream adapter backed by an unbounded mpsc channel carrying `Result<T, Status>` items — used for
/// server-streaming RPCs (e.g. `StreamExecuteTool`) whose frames may carry a mid-stream status. The
/// one definition is `tddy-worktree-service`'s; this path stays for the callers that name it here.
pub use tddy_worktree_service::stream::MpscResultStream;

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
pub struct DaemonSessionHost {
    config: DaemonConfig,
    #[allow(dead_code)]
    // Kept for API compatibility; callers pass a resolver but tddy_data_dir is used directly.
    sessions_base_for_user: tddy_daemon_kernel::SessionsBaseResolver,
    tddy_data_dir: PathBuf,
    user_resolver: tddy_daemon_kernel::SessionUserResolver,
    spawn_client: Option<Arc<spawn_worker::SpawnClient>>,
    /// The peers this daemon may route a request to and the common-room slot it forwards through —
    /// shared with the families served above this crate, which route against the same roster.
    peer_routing: crate::peer_routing::PeerRouting,
    /// Where each presenter event of a workflow session goes besides the notification bus — the
    /// Telegram chat surface, on a daemon that has one. `None` when no chat surface is configured.
    presenter_event_sink: Option<tddy_daemon_kernel::presenter_observer::SharedPresenterEventSink>,
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
    /// The relay's idle tracker, when it has one — bumped on every RPC call, here and by the
    /// families served above this crate, which hold the same one.
    rpc_activity: crate::relay_idle::RpcActivity,
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
    /// Each operator's open credential vault, where the GitHub access token their login granted is
    /// sealed — the credential the PR-status reads act with. `None` (no `auth_storage` configured)
    /// means a real login's PR status reads as *unavailable*, never as "no PR".
    credential_vaults: Option<Arc<tddy_daemon_auth::SessionVaults>>,
    /// This daemon's session-token signer and the verifier for every daemon's tokens — the same
    /// value `tddy_daemon_auth::build_auth_entries_with` built the RPC gate from. What mints an
    /// agent's own credential for a split or jailed-codebase session. `None` means this daemon
    /// signs nothing, and a placement that needs a minted credential is refused.
    session_tokens: Option<tddy_daemon_auth::SessionTokens>,
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
    /// Sandbox-IPC bridge installed once the top-level `Arc` exists (`runtime::build`).
    sandbox_rpc_bridge: Arc<std::sync::OnceLock<Arc<dyn tddy_sandbox_runner::HostRpcHandler>>>,
    /// The RPC families served above this crate, installed last by the composition root with
    /// [`Self::with_rpc_families`]. `None` until then, and [`Self::rpc_families`] refuses rather
    /// than serving a room without them — see [`crate::rpc_families`], which owns it (hence
    /// `pub(crate)`).
    pub(crate) rpc_families: Option<Arc<dyn crate::rpc_families::DaemonRpcFamilies>>,
}

mod seed_codebase;
pub use seed_codebase::*;

mod stack_parent;
pub use stack_parent::*;

mod seeded_clone_guard;
pub use seeded_clone_guard::*;

mod svc_resolve_tddy_tools_path;
pub use svc_resolve_tddy_tools_path::resolve_tddy_tools_path;

mod svc_pr_status_for_caller;

mod svc_start_claude_cli_session;

mod hooks_and_urls;
pub use hooks_and_urls::*;

mod agent_roster;
pub(crate) use agent_roster::*;
/// Shared with `tddy-daemon-rpc`'s `ListSubagents`, whose rows name agents the way the roster does.
pub use agent_roster::{def_tool_names, qualified_agent_id};

mod claude_cli_spawn;
pub(crate) use claude_cli_spawn::*;

mod svc_resolve_listed_worktree;
pub use svc_resolve_listed_worktree::resolvable_agent_defs;

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

mod managed_launch;
pub(crate) use managed_launch::*;

mod stack_child_spawn;
pub(crate) use stack_child_spawn::*;

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

mod conversation_spawn;
pub(crate) use conversation_spawn::*;

mod conversation_spawn_handler;

mod roster_replacement;
pub use roster_replacement::*;

mod attachment_progress;
pub(crate) use attachment_progress::*;

mod placement;
pub use placement::*;

mod split_start;
pub use split_start::*;

mod svc_resolve_os_user;
/// Caller identity, shared with `tddy-daemon-rpc`'s exec-tool and PR-stack families, which must
/// authenticate a caller exactly as the host does.
pub use svc_resolve_os_user::{
    authorize_exec_tool_caller, resolve_exec_tool_worktree, resolve_os_user,
};

/// Where an exec tool runs on this daemon, shared with `tddy-daemon-rpc`'s exec-tool family.
mod local_exec_tools;
pub use local_exec_tools::LocalExecTools;

mod svc_materialize_staged_attachment;

mod svc_spawn_split_agent;

mod svc_shut_down_children;

mod svc_start_sandboxed_codebase_session;

mod svc_start_session_core;

mod svc_terminal_ports;

mod svc_session_files_ports;

/// The daemon's half of `activity.ActivityService` — the six host answers families M and N read,
/// and the routing the daemon keeps. `#unbundle` node 7.
mod svc_activity_ports;

mod family_proto_bridge;
/// Shared with the families served in `tddy-daemon-rpc`, whose bodies bridge the same
/// wire-identical messages.
pub use family_proto_bridge::wire_same;
/// The host state `tddy-daemon-rpc`'s family handlers are built from.
mod handler_state;
mod session_coordinate_handlers;
/// The daemon's half of `session_agents.SessionAgentService` — the host capabilities family B
/// reads, and the routing the daemon keeps. `#unbundle` node 7.
mod svc_session_agent_ports;
mod svc_session_lifecycle_ports;

pub use svc_session_files_ports::PeerRoutedSessionFiles;

/// Node 7's two served surfaces, named because the local Unix socket mounts them: the bundle
/// `local_socket_server` takes is generic over the implementation each generated adapter wraps, so
/// the host that assembles it has to be able to write these two types down.
pub use svc_activity_ports::PeerRoutedActivity;
pub use svc_session_agent_ports::PeerRoutedSessionAgents;

/// The host-side RPC dispatch for a sandboxed session's `SessionChannel`: routes the roster and
/// conversation RPCs the in-jail `tddy-tools` issues (forwarded by the runner as `RpcRequest`s)
/// to this daemon's `DaemonSessionHost`. The runner's `ToolExecService` forwards
/// `StreamSessionAgents` / `OpenAgentConversation` / `PromptAgentConversation` /
/// `CancelAgentConversation` / `ReportAgentConversationState`; this handler decodes each, calls the
/// matching typed method on the
/// `Arc<DaemonSessionHost>` it holds, and returns the encoded response — unary for the two
/// unary RPCs, a server stream of encoded frames for the two streaming ones. `tonic::Status`
/// errors are carried back to the in-jail caller as a single terminal `RpcStreamFrame` with
/// `error` set, which the runner's relay turns into the `tddy_rpc::Status` the caller sees.
///
/// The reference back to the host is **weak**. `sandbox_rpc_bridge` is a field *of*
/// `DaemonSessionHost`, so a strong `Arc` here would close a cycle the host could never escape:
/// the daemon would never drop, its `WorkspaceSandboxRegistry` would never drop, and every jail
/// it holds would outlive it as an orphaned `tddy-sandbox-runner` on the host. The host owns the
/// bridge; the bridge only borrows the host.
struct DaemonRpcHandler {
    conn: std::sync::Weak<DaemonSessionHost>,
}

impl DaemonRpcHandler {
    /// The daemon this bridge serves, or the refusal to answer with when it is gone.
    ///
    /// An upgrade that fails is not a transient condition to retry or to paper over: the daemon
    /// that owned the jail has been dropped, so there is no host left to serve the call and no
    /// safe place to serve it from instead. The caller is told so explicitly rather than handed a
    /// silent empty answer it would read as "no agents".
    fn host(&self) -> Result<Arc<DaemonSessionHost>, tddy_rpc::Status> {
        self.conn.upgrade().ok_or_else(|| {
            tddy_rpc::Status::unavailable(
                "the daemon that owns this sandboxed session has been shut down; its host RPC \
                 bridge cannot serve calls from inside the jail any more",
            )
        })
    }
}

mod daemon_rpc_handler;

mod demo_vm_coordinate_handlers;
mod svc_demo_vm_ports;
pub use svc_demo_vm_ports::DemoVmServiceImpl;

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
/// that false. One assert covers `tddy-daemon-rpc`'s `EXEC_TOOL_FRAME_BYTES` too, which is defined as
/// this same constant.
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
/// The framing is [`tddy_session_activity::service::activity_delta_frames`], the one the activity
/// service streams with; this takes the session room's own delta type. Every frame carries the whole
/// description, and a call that changed nothing is **one** frame with an empty patch — AC9.
pub fn activity_delta_frames(delta: &ActivityDelta) -> Vec<AgentActivityDeltaChunk> {
    tddy_session_activity::service::activity_delta_frames(&svc_activity_ports::measured_delta(
        delta.clone(),
    ))
}

mod stack_seed_validation;
pub use stack_seed_validation::*;

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

/// A spawned child must record its **branch** on the planned node it materializes. Without that
/// forward link the orchestrator's stack still reads "no branch anywhere", so `base_ref_for_spawn`
/// refuses every descendant — a stack wedged at its bottom node. The child session id is recorded
/// alongside it only as a fallback route back to the branch.
#[cfg(test)]
mod stack_child_link_tests;

#[cfg(test)]
mod start_session_binary_resolution_tests;

#[cfg(test)]
mod resume_session_binary_resolution_tests;

mod worktree_source;
pub use worktree_source::*;

#[cfg(test)]
mod worktree_source_tests;

#[cfg(test)]
mod sandbox_claude_passthrough_args_tests;

#[cfg(test)]
mod conversation_spawn_wiring_tests;

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
/// [`DaemonSessionHost::local_agent_codebase_access`] is the seam under test and it is private:
/// widening it to `pub` purely so a test could call it would export an internal for no other
/// caller. This is the roster half of the dispatch contract; the remote-caller half is proven from
/// the outside, over the RPC surface.
#[cfg(test)]
mod workspace_sandbox_roster_dispatch_unit_tests;
