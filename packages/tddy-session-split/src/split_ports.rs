//! What the split topic needs from the session host that is not one of its fields, and the owned
//! handle that carries both.
//!
//! A split session is an agent paired with a codebase on a sandboxed checkout: its LiveKit room,
//! its context read from the codebase host, its teardown. Those read a fixed set of host fields
//! (held by [`SplitSessions`]) and reach five capabilities that are not fields and live in wiring
//! above this topic: [`SplitHost`] is how the topic reaches them without naming the host. It
//! extends [`AgentHostCallbacks`], so the agent topic's callbacks (`worktree_snapshot` among them)
//! come through the same handle.
//!
//! Its fields carry the host's names, so a method that moves from `impl DaemonSessionHost` to
//! `impl SplitSessions` changes its `impl` header and nothing in the body. This follows
//! [`AgentRoster`](super::agent_host_callbacks::AgentRoster) exactly.

use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::peer_routing::PeerRouting;
use tddy_daemon_livekit::session_room::{
    RemoteSnapshotSource, SessionRoomRegistry, WorktreeSnapshot,
};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{
    DeleteSessionRequest, DeleteSessionResponse, StartSessionRequest, StartSessionResponse,
};
use tddy_service::proto::session_agents_svc::SessionAgentService;
use tddy_service::proto::session_agents_svc::{AgentConversationChunk, SessionAgentRoster};
use tddy_service::proto::session_files::{
    ContextFileBatchChunk, ContextFileChunk, ContextManifestEntry, HostDocumentChunk,
    SessionFilesService,
};
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_worktree_service::stream::MpscResultStream;

use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_session_agents::agent_host_callbacks::{AgentHostCallbacks, AgentRoster};
use tddy_session_files::svc_materialize_staged_attachment::AttachmentState;

/// This daemon's `session_files.SessionFilesService` surface, as the split topic reads context
/// through it: routed, so a read naming another daemon is served by that daemon.
pub type SplitSessionFiles = dyn SessionFilesService<
    StreamContextManifestStream = MpscResultStream<ContextManifestEntry>,
    StreamReadContextFileStream = MpscResultStream<ContextFileChunk>,
    StreamReadContextFileBatchStream = MpscResultStream<ContextFileBatchChunk>,
    StreamReadHostDocumentStream = MpscResultStream<HostDocumentChunk>,
>;

/// This daemon's `session_agents.SessionAgentService` surface, as the split topic reads a roster
/// through it.
pub type SplitSessionAgents = dyn SessionAgentService<
    StreamSessionAgentsStream = MpscResultStream<SessionAgentRoster>,
    PromptAgentConversationStream = MpscResultStream<AgentConversationChunk>,
    ResumeAgentConversationStream = MpscResultStream<AgentConversationChunk>,
>;

/// The capabilities of the session host the split topic calls and does not own.
///
/// Implemented once, on the host, in wiring (`svc_agent_host_ports`).
#[async_trait::async_trait]
pub trait SplitHost: AgentHostCallbacks {
    /// Start the `workspace` session that holds a sandboxed codebase's checkout: the host's own
    /// session start, reached from here because the launch topic sits above this one.
    async fn start_workspace_session(
        &self,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status>;

    /// Delete a session through the handler an operator's `DeleteSession` reaches, which is what
    /// stops its jail and removes its worktree.
    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status>;

    /// This daemon's session-file surface, which a context read is served at whichever host holds
    /// the codebase.
    fn session_files(&self) -> Arc<SplitSessionFiles>;

    /// This daemon's roster surface, which a split agent's withdrawals are read back through.
    fn session_agents(&self) -> Arc<SplitSessionAgents>;

    /// Every coordinate a session room serves (the host's `session_room_roster`).
    fn session_room_roster(&self) -> Result<tddy_rpc::MultiRpcService, Status>;
}

/// The session host's split fields, owned, plus its callbacks.
///
/// Built per call by the host (`DaemonSessionHost::split_sessions`). Every shared field is the
/// `Arc` the host holds, so a clone of this talks to the registries, rooms and peers the host does.
#[derive(Clone)]
pub struct SplitSessions {
    pub config: DaemonConfig,
    pub tddy_data_dir: std::path::PathBuf,
    pub staging_base_dir: std::path::PathBuf,
    pub peer_routing: PeerRouting,
    pub session_rooms: Arc<SessionRoomRegistry>,
    pub workspace_sandboxes:
        Arc<tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxRegistry>,
    pub workspace_sandbox_provisioner:
        Arc<dyn tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxProvisioner>,
    pub claude_cli_manager: Arc<CliSessionManager>,
    pub session_tokens: Option<tddy_daemon_auth::SessionTokens>,
    /// The agent topic's handle: the split start resolves the request's agent defs through it.
    pub agent_roster: AgentRoster,
    /// The host's capabilities that are not fields.
    pub host: Arc<dyn SplitHost>,
}

impl SplitSessions {
    /// The signer and verifier an agent's own credential is minted with, or the refusal a daemon
    /// that signs nothing gives.
    pub(crate) fn session_tokens(&self) -> Result<&tddy_daemon_auth::SessionTokens, Status> {
        self.session_tokens.as_ref().ok_or_else(|| {
            Status::failed_precondition(
                "this daemon signs no session tokens, so an agent's tool calls could not be \
                 authenticated: configure `github:` — which gives the daemon its signing \
                 identity — and retry",
            )
        })
    }

    /// The fields attachment materialization reads, lent to it for the length of one call.
    pub(crate) fn attachment_state(&self) -> AttachmentState<'_> {
        AttachmentState {
            config: &self.config,
            tddy_data_dir: &self.tddy_data_dir,
            staging_base_dir: &self.staging_base_dir,
            peer_routing: &self.peer_routing,
        }
    }

    /// The host as the source a room's poll loop measures a peer's checkout with: one
    /// `worktree_snapshot` callback per poll.
    pub(crate) fn remote_worktree_snapshots(&self) -> Arc<dyn RemoteSnapshotSource> {
        Arc::new(HostWorktreeSnapshots {
            host: Arc::clone(&self.host),
        })
    }
}

/// [`AgentHostCallbacks::worktree_snapshot`] as a [`RemoteSnapshotSource`].
struct HostWorktreeSnapshots {
    host: Arc<dyn SplitHost>,
}

#[async_trait::async_trait]
impl RemoteSnapshotSource for HostWorktreeSnapshots {
    async fn snapshot(
        &self,
        session_token: &str,
        codebase_session_id: &str,
        codebase_instance_id: &str,
    ) -> Result<WorktreeSnapshot, Status> {
        self.host
            .worktree_snapshot(session_token, codebase_session_id, codebase_instance_id)
            .await
    }
}
