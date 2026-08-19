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
use tddy_core::{BranchWorktreeIntent, Changeset};
use tddy_rpc::{Request, Response, Status, Streaming};
use tddy_service::proto::connection::{
    session_attachment::Source as AttachmentSource,
    start_session_event::Event as StartSessionEventKind, AttachmentMaterializationProgress,
    HostDocumentChunk, HostDocumentRef, HostDocumentScope, ReadHostDocumentRequest,
    ReadHostDocumentResponse, SessionAttachment, StagedAttachmentEntry, StagedAttachmentRef,
    StartSessionEvent,
};
use tddy_service::proto::connection::{
    AddPlannedPrRequest, AddPlannedPrResponse, AddProjectToHostRequest, AddProjectToHostResponse,
    AgentConversationChunk, AgentInfo, AttachSessionAgentRequest, BranchConflict,
    CalculateWorktreeSizeRequest, CalculateWorktreeSizeResponse, CancelAgentConversationRequest,
    CancelAgentConversationResponse, ClaimTerminalControlRequest, ClaimTerminalControlResponse,
    CleanWorktreeRequest, CleanWorktreeResponse, ConnectSessionRequest, ConnectSessionResponse,
    ConnectionService as ConnectionServiceTrait, CreateProjectRequest, CreateProjectResponse,
    DeleteSessionRequest, DeleteSessionResponse, DeleteSessionUploadRequest,
    DeleteSessionUploadResponse, DeleteStagedAttachmentRequest, DeleteStagedAttachmentResponse,
    DetachSessionAgentRequest, EligibleDaemonEntry, ListAgentModelsRequest,
    ListAgentModelsResponse, ListAgentsRequest, ListAgentsResponse, ListEligibleDaemonsRequest,
    ListEligibleDaemonsResponse, ListProjectBranchesRequest, ListProjectBranchesResponse,
    ListProjectsRequest, ListProjectsResponse, ListSessionAgentsRequest, ListSessionUploadsRequest,
    ListSessionUploadsResponse, ListSessionWorkflowFilesRequest, ListSessionWorkflowFilesResponse,
    ListSessionsRequest, ListSessionsResponse, ListStagedAttachmentsRequest,
    ListStagedAttachmentsResponse, ListSubagentsRequest, ListSubagentsResponse,
    ListTerminalSessionsRequest, ListTerminalSessionsResponse, ListToolsRequest, ListToolsResponse,
    ListWorktreeDirectoryRequest, ListWorktreeDirectoryResponse, ListWorktreesForProjectRequest,
    ListWorktreesForProjectResponse, MintLocalTokenRequest, MintLocalTokenResponse, ModelInfo,
    OpenAgentConversationRequest, OpenAgentConversationResponse, ProjectEntry as ProtoProjectEntry,
    PromptAgentConversationRequest, ReadSessionWorkflowFileRequest,
    ReadSessionWorkflowFileResponse, ReadWorktreeFileRequest, ReadWorktreeFileResponse,
    RemoveWorktreeRequest, RemoveWorktreeResponse, ReportSessionStatusRequest,
    ReportSessionStatusResponse, RestoreSessionWorktreeRequest, RestoreSessionWorktreeResponse,
    ResumeSessionRequest, ResumeSessionResponse, SendTerminalInputResponse, SessionAgentRoster,
    SessionEntry as ProtoSessionEntry, SessionTerminalInput, SessionTerminalOutput,
    SessionUploadEntry, SetProjectDefaultBranchRequest, SetProjectDefaultBranchResponse, Signal,
    SignalSessionRequest, SignalSessionResponse, StartSessionRequest, StartSessionResponse,
    StartTerminalSessionRequest, StartTerminalSessionResponse, StopTerminalSessionRequest,
    StopTerminalSessionResponse, StreamSessionAgentsRequest, StreamTerminalOutputRequest,
    StreamWorktreeStatsRequest, SubagentInfo, TerminalControlEvent, TerminalHistoryChunk,
    TerminalSessionInfo, ToolInfo, UploadSessionFileChunkRequest, UploadSessionFileChunkResponse,
    UploadStagedAttachmentChunkRequest, UploadStagedAttachmentChunkResponse,
    WatchTerminalControlRequest, WorkflowFileEntry, WorktreeDirEntry, WorktreeRow,
    WorktreeSizeStatus as ProtoWorktreeSizeStatus, WorktreeStatsEvent,
};
use tddy_terminal_rpc::TerminalSessionStore;
use uuid::Uuid;

use crate::agent_list_mapping::agent_allowlist_rows;
use crate::branch_intent::{
    resolve_branch_workflow, BranchIntentPolicy, BranchIntentRequest, ResolvedBranchWorkflow,
};
use crate::cli_session_manager::{ClaimOutcome, CliSessionManager, MAIN_TERMINAL_ID};
use crate::config::DaemonConfig;
use crate::host_stats::{HostStats, SysinfoHostStats};
use crate::livekit_peer_discovery::{
    local_instance_id_for_config, LiveKitDiscoveryHandles, PeerRoute,
};
use crate::livekit_rooms_stream::{pump_rooms, room_roster_from_config, RoomRoster};
use crate::multi_host::{EligibleDaemonSource, StubEligibleDaemonSource};
use crate::project_storage::{self, ProjectData};
use crate::session_attachments::validate_attachment_basename;
use crate::session_deletion;
use crate::session_file_upload::{contained_canonical_dir, validate_segment};
use crate::session_list_enrichment;
use crate::session_reader;
use crate::session_room::{ActivityDelta, DeltaLookupError, DeltaScope};
use crate::spawn_worker;
use crate::spawner::{self, SpawnOptions};
use crate::telegram_session_subscriber::TelegramDaemonHooks;
use crate::tool_engine;
use crate::user_sessions_path::{
    project_path_under_home_from_user_relative, projects_path_for_user, repos_base_for_user,
};
use crate::workspace_session;
use crate::worktrees::{
    self, CleanWorktreeError, RemoveWorktreeError, WorktreeDiffRow, WorktreeSizeCalculator,
    WorktreeSizeStatus, WorktreeStatsCache,
};
use tddy_service::proto::connection::{
    AcpReplayFrame, AgentActivityDeltaChunk, AgentActivityDeltaRequest,
    AgentActivityRecord as ProtoAgentActivityRecord, DeltaScope as ProtoDeltaScope, DemoVmState,
    ExecuteToolChunk, ExecuteToolRequest, ExecuteToolResponse, GetAcpReplayPageRequest,
    GetAcpReplayPageResponse, GetAcpToolCallDetailRequest, GetAcpToolCallDetailResponse,
    GetDemoVmStatusRequest, GetDemoVmStatusResponse, GetPrStatusRequest, GetPrStatusResponse,
    GetTerminalHistoryRequest, GetWorktreeSnapshotRequest, GetWorktreeSnapshotResponse,
    HostCpuStats, HostDiskStats, HostStatsEvent, ListExecToolsRequest, ListExecToolsResponse,
    ListSessionToolCallsRequest, ListSessionToolCallsResponse, LiveKitRoomsEvent,
    PullBaseIntoBranchRequest, PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse,
    ReorderPlannedPrRequest, ReorderPlannedPrResponse, RepointPlannedPrRequest,
    RepointPlannedPrResponse, ReportAgentActivityRequest, ReportAgentActivityResponse,
    StartDemoVmRequest, StartDemoVmResponse, StopDemoVmRequest, StopDemoVmResponse,
    StreamAcpReplayRequest, StreamHostStatsRequest, StreamLiveKitRoomsRequest, StreamMode,
    StreamSessionActivityRequest, ToolCallInfo as ProtoToolCallInfo, WorktreeFileChunk,
};
use tddy_task::{TaskRegistry, TerminalCapture};

/// Runs blocking clone/spawn work with a wall-clock cap so hung NSS/git/spawn cannot block RPCs forever.
pub(crate) async fn spawn_blocking_with_timeout<T: Send + 'static>(
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

/// Await a `tddy-supervisor`-brokered operation under the same deadline the forked spawn backend
/// gets from [`spawn_blocking_with_timeout`].
///
/// An unreachable or refusing supervisor fails the RPC. There is deliberately no local spawn to fall
/// back to: doing the work here would run a session as the daemon's own user, which is the isolation
/// the supervisor exists to provide.
pub(crate) async fn await_supervised_with_timeout<T>(
    timeout: Duration,
    op_label: &'static str,
    operation: impl std::future::Future<Output = anyhow::Result<T>>,
) -> Result<T, Status> {
    match tokio::time::timeout(timeout, operation).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(e)) => {
            log::error!("{} failed: {:#}", op_label, e);
            Err(Status::internal(format!("{e:#}")))
        }
        Err(_elapsed) => {
            log::error!(
                "{} timed out after {}s (spawn_worker_request_timeout_secs) waiting for tddy-supervisor",
                op_label,
                timeout.as_secs()
            );
            Err(Status::deadline_exceeded(format!(
                "{}: tddy-supervisor did not answer within {}s",
                op_label,
                timeout.as_secs()
            )))
        }
    }
}

/// After a `new_branch_from_base` worktree is created, optionally push the freshly created branch to
/// its remote. Reads the actual created branch from the session's changeset (it may carry a
/// collision suffix), resolves the remote from the persisted integration base ref
/// (`<remote>/<branch>`) — falling back to main-worktree detection then `origin` — runs
/// `git push -u <remote> <branch>` from the worktree, and records `Changeset.remote_pushed = true`.
/// A push failure fails the session start — no silent fallback.
pub(crate) async fn push_new_branch_to_origin_if_requested(
    create_remote_branch: bool,
    intent: BranchWorktreeIntent,
    session_dir: &Path,
    worktree_path: &Path,
    timeout: Duration,
) -> Result<(), Status> {
    if !create_remote_branch || !matches!(intent, BranchWorktreeIntent::NewBranchFromBase) {
        return Ok(());
    }
    let session_dir = session_dir.to_path_buf();
    let worktree_path = worktree_path.to_path_buf();
    spawn_blocking_with_timeout(
        timeout,
        "StartSession: push new branch to remote",
        move || {
            let mut cs = tddy_core::read_changeset(&session_dir)
                .map_err(|e| anyhow::anyhow!("read changeset for remote push: {e}"))?;
            let branch = cs
                .branch
                .clone()
                .ok_or_else(|| anyhow::anyhow!("no branch recorded after worktree setup"))?;
            // Resolve the remote from the persisted integration base ref (`<remote>/<branch>`),
            // falling back to main-worktree detection then `origin` as the last resort.
            let remote = cs
                .effective_worktree_integration_base_ref
                .as_deref()
                .and_then(|r| r.split_once('/').map(|(remote, _)| remote.to_string()))
                .or_else(|| tddy_core::worktree::detect_default_remote_name(&worktree_path))
                .unwrap_or_else(|| "origin".to_string());
            tddy_core::worktree::push_new_branch_to_remote(&worktree_path, &branch, &remote)
                .map_err(|e| anyhow::anyhow!(e))?;
            cs.remote_pushed = true;
            tddy_core::write_changeset(&session_dir, &cs)
                .map_err(|e| anyhow::anyhow!("write changeset after remote push: {e}"))?;
            Ok(())
        },
    )
    .await
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

/// Maximum size of a single terminal-output frame published to a client on attach. Chosen to stay
/// well under the LiveKit/WebRTC data-channel and gRPC-web message size limits while keeping the
/// number of replay frames for a long-lived session reasonable.
pub(crate) const TERMINAL_OUTPUT_FRAME_MAX_BYTES: usize = 32 * 1024;

/// Split a terminal capture buffer into ordered frames of at most `max_frame_bytes` each so a long
/// session history is replayed as several bounded frames instead of one oversized frame that could
/// exceed the transport's per-message limit and never reach the client.
///
/// An empty input yields no frames. Any non-empty input yields `ceil(len / max_frame_bytes)`
/// frames; concatenating them in order reproduces the input exactly.
///
/// Retained for the `sandbox_replay_tests` unit tests (the production sandbox path now uses
/// `TerminalCapture::replay_from` directly with offset-tagged frames).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn chunk_terminal_output(data: &[u8], max_frame_bytes: usize) -> Vec<bytes::Bytes> {
    data.chunks(max_frame_bytes)
        .map(bytes::Bytes::copy_from_slice)
        .collect()
}

/// Frames a newly attached sandbox-session subscriber receives before the live broadcast: the
/// mouse-tracking modes still in effect, then the retained output.
///
/// Without the prologue a browser attaching to a long-running sandbox session never learns the
/// application enabled mouse reporting, because the DECSET that enabled it was evicted from the
/// capture ring long ago and nothing re-emits it.
///
/// Retained for the `sandbox_replay_tests` unit tests (the production sandbox path now uses
/// `TerminalCapture::replay_from` directly with offset-tagged frames).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn sandbox_replay_frames(
    capture: &TerminalCapture,
    max_frame_bytes: usize,
) -> Vec<bytes::Bytes> {
    chunk_terminal_output(&capture.replay(), max_frame_bytes)
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
    /// The session and terminal the broadcast belongs to — stamped on every frame this adapter
    /// yields, since a client cannot tell whose bytes an unidentified frame carries.
    identity: TerminalFrameIdentity,
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
                    return std::task::Poll::Ready(Some(Ok(self
                        .identity
                        .data_frame(chunk.to_vec()))));
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
    rx: tokio::sync::mpsc::UnboundedReceiver<SessionTerminalOutput>,
}

impl Stream for MpscTerminalOutputStream {
    type Item = Result<SessionTerminalOutput, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.rx.poll_recv(cx) {
            std::task::Poll::Ready(Some(msg)) => std::task::Poll::Ready(Some(Ok(msg))),
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

/// The session and terminal a stream's frames belong to. Every frame carries it, so a client can
/// tell its own terminal's bytes from another terminal's and drop what is not its own instead of
/// silently painting it. `terminal_id` is always the RESOLVED id (an empty request id resolves to
/// the reserved main terminal), matching `tddy_terminal_rpc::bridge`.
#[derive(Clone)]
struct TerminalFrameIdentity {
    session_id: String,
    terminal_id: String,
}

impl TerminalFrameIdentity {
    fn new(session_id: &str, terminal_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            terminal_id: resolved_terminal_id(terminal_id).to_string(),
        }
    }

    /// A terminal output-data frame (no ACK).
    fn data_frame(&self, data: Vec<u8>) -> SessionTerminalOutput {
        SessionTerminalOutput {
            data,
            acked_input_offset: 0,
            session_id: self.session_id.clone(),
            terminal_id: self.terminal_id.clone(),
            ..Default::default()
        }
    }

    /// A replay / catch-up frame tagged with its absolute byte offsets and whether it reaches the
    /// capture ring's oldest retained byte. Used by the sandbox path so a reconnecting client can
    /// resume by offset (FROM_OFFSET) instead of re-receiving the whole retained buffer.
    fn replay_frame(
        &self,
        data: Vec<u8>,
        start_offset: u64,
        end_offset: u64,
        at_oldest: bool,
    ) -> SessionTerminalOutput {
        SessionTerminalOutput {
            data,
            acked_input_offset: 0,
            start_offset,
            end_offset,
            at_oldest,
            session_id: self.session_id.clone(),
            terminal_id: self.terminal_id.clone(),
        }
    }
}

/// Convert a daemon `connection::SessionTerminalInput` (tonic ConnectionService proto) into the
/// bridge's `terminal_session::SessionTerminalInput` so the bidi handler can route through the
/// shared bridge helper. The two protos carry identical fields; this is a structural copy.
fn to_bridge_terminal_input(
    msg: &SessionTerminalInput,
) -> tddy_terminal_rpc::proto::terminal_session::SessionTerminalInput {
    tddy_terminal_rpc::proto::terminal_session::SessionTerminalInput {
        session_token: msg.session_token.clone(),
        session_id: msg.session_id.clone(),
        data: msg.data.clone(),
        terminal_id: msg.terminal_id.clone(),
        control_token: msg.control_token.clone(),
        input_offset: msg.input_offset,
        mode: msg.mode,
        from_offset: msg.from_offset,
        initial_cols: msg.initial_cols,
        initial_rows: msg.initial_rows,
    }
}

/// Convert a bridge `terminal_session::SessionTerminalOutput` (carrying offset metadata) into the
/// daemon's `connection::SessionTerminalOutput` for the tonic/RpcService stream.
fn to_connection_output(
    out: tddy_terminal_rpc::proto::terminal_session::SessionTerminalOutput,
) -> SessionTerminalOutput {
    SessionTerminalOutput {
        data: out.data,
        acked_input_offset: out.acked_input_offset,
        start_offset: out.start_offset,
        end_offset: out.end_offset,
        at_oldest: out.at_oldest,
        // The bridge stamped the frame with the session and resolved terminal it came from; carry
        // that identity through so the client can drop output that is not its own.
        session_id: out.session_id,
        terminal_id: out.terminal_id,
    }
}

impl Unpin for MpscTerminalOutputStream {}

/// Stream adapter backed by an unbounded mpsc channel carrying `Result<T, Status>` items — used for
/// server-streaming RPCs (e.g. `GetTerminalHistory`) whose frames may carry a mid-stream status.
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
    manager: Arc<crate::cli_session_manager::CliSessionManager>,
    mut broadcast_rx: tokio::sync::broadcast::Receiver<
        crate::cli_session_manager::ControlChangeEvent,
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

/// Stream adapter backed by an mpsc channel for [`ProtoAgentActivityRecord`] server-streaming.
pub struct MpscAgentActivityStream {
    rx: tokio::sync::mpsc::UnboundedReceiver<ProtoAgentActivityRecord>,
}

impl Stream for MpscAgentActivityStream {
    type Item = Result<ProtoAgentActivityRecord, Status>;

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

impl Unpin for MpscAgentActivityStream {}

/// Stream adapter backed by an mpsc channel for [`AcpReplayFrame`] server-streaming.
pub struct MpscAcpReplayStream {
    rx: tokio::sync::mpsc::UnboundedReceiver<AcpReplayFrame>,
}

impl Stream for MpscAcpReplayStream {
    type Item = Result<AcpReplayFrame, Status>;

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

impl Unpin for MpscAcpReplayStream {}

/// Wrap one ACP frame in the connection-local [`AcpReplayFrame`] envelope, encoding the inner
/// `AcpAgentMessage` to its protobuf bytes and stamping its absolute transcript position.
///
/// `seq` is the frame's 0-based index in the session's *resolved* transcript
/// ([`tddy_service::acp_replay::read_session_transcript`]) — the same list
/// [`tddy_service::acp_replay::page_before`] indexes, so the reverse cursor a client reads off a
/// frame addresses the same position the pager does.
fn acp_replay_frame(frame: &tddy_service::proto::acp::AcpAgentMessage, seq: u64) -> AcpReplayFrame {
    AcpReplayFrame {
        acp_agent_message: tddy_service::acp_replay::strip_tool_body(frame).encode_to_vec(),
        // A transcript frame carries no count; the count-first mode sets this instead.
        activity_count: 0,
        seq,
    }
}

/// Relay task for `StreamAcpReplay`: forwards live agent-activity records for one session (the
/// broadcast is already session-scoped) as enriched ACP `tool_call` replay frames into `tx` until
/// the client disconnects.
///
/// `next_seq` is the resolved transcript's length at subscribe time, so the live tail continues the
/// snapshot's numbering and a frame delivered live carries the position a later re-read would give
/// it.
///
/// A tool call broadcasts twice — its `running` record then its terminal one — but the two coalesce
/// into a *single* resolved transcript entry, so the refinement must land on the position its first
/// record was given instead of consuming one of its own. `seq_by_tool_call` remembers that mapping
/// and is pre-seeded from the snapshot ([`seq_by_tool_call`]), so a call straddling the subscribe
/// boundary refines the entry the snapshot already placed.
async fn relay_acp_replay(
    mut broadcast_rx: tokio::sync::broadcast::Receiver<
        tddy_core::agent_activity::AgentActivityRecord,
    >,
    tx: tokio::sync::mpsc::UnboundedSender<AcpReplayFrame>,
    mut next_seq: u64,
    mut seq_by_tool_call: std::collections::HashMap<String, u64>,
) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match broadcast_rx.recv().await {
            Ok(record) => {
                let frame = tddy_service::acp_replay::frame_for_agent_activity(&record);
                let seq = match tddy_service::acp_replay::tool_call_id_of(&frame) {
                    Some(id) => *seq_by_tool_call.entry(id.to_string()).or_insert_with(|| {
                        let seq = next_seq;
                        next_seq += 1;
                        seq
                    }),
                    None => {
                        let seq = next_seq;
                        next_seq += 1;
                        seq
                    }
                };
                if tx.send(acp_replay_frame(&frame, seq)).is_err() {
                    break;
                }
            }
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

/// The absolute 0-based position of every tool call in a resolved transcript, keyed by
/// `tool_call_id`.
///
/// Seeds [`relay_acp_replay`]'s live numbering: when a call whose `running` record is already in the
/// snapshot reports its terminal record, that record refines the snapshot entry and so must carry
/// the snapshot's position for it — not a fresh position at the tail.
fn seq_by_tool_call(
    frames: &[tddy_service::proto::acp::AcpAgentMessage],
) -> std::collections::HashMap<String, u64> {
    frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| {
            tddy_service::acp_replay::tool_call_id_of(frame)
                .map(|id| (id.to_string(), index as u64))
        })
        .collect()
}

/// Count-only relay task for `StreamAcpReplay`'s `CountThenLive` mode: each **newly-seen** tool call
/// published to the session hub bumps `count` by one and emits a fresh count-only `AcpReplayFrame`
/// (no transcript payload) into `tx`, until the client disconnects. A call's `running` and terminal
/// records share a `call_id` and so count once (matching the coalesced rows the pane renders);
/// `seen_ids` is pre-seeded with the snapshot's ids so a call straddling the subscribe boundary is
/// not double-counted. This is the cheap feed that drives the overlay's activity badge before the
/// full pane is opened.
async fn relay_acp_replay_count(
    mut broadcast_rx: tokio::sync::broadcast::Receiver<
        tddy_core::agent_activity::AgentActivityRecord,
    >,
    tx: tokio::sync::mpsc::UnboundedSender<AcpReplayFrame>,
    mut count: u64,
    mut seen_ids: std::collections::HashSet<String>,
) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match broadcast_rx.recv().await {
            Ok(record) => {
                if !seen_ids.insert(record.call_id) {
                    // A record for a call already counted (its terminal row, or a snapshot straddler).
                    continue;
                }
                count += 1;
                if tx
                    .send(AcpReplayFrame {
                        acp_agent_message: Vec::new(),
                        activity_count: count,
                        // A count frame carries no transcript payload, so it has no position.
                        seq: 0,
                    })
                    .is_err()
                {
                    break;
                }
            }
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

/// Default cadence for refreshing per-core CPU utilization on the host-stats sampling loop.
const HOST_CPU_INTERVAL: Duration = Duration::from_secs(5);
/// Default cadence for refreshing project-dir disk figures on the host-stats sampling loop.
const HOST_DISK_INTERVAL: Duration = Duration::from_secs(60);

/// Cadence at which a `StreamLiveKitRooms` subscription re-reads the LiveKit roster. Presence is
/// the volatile fact on that panel, hence far shorter than the host-stats disk tick.
const LIVEKIT_ROOMS_POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Stream adapter backed by an mpsc channel for [`LiveKitRoomsEvent`] server-streaming.
///
/// Carries results rather than events: a roster read that fails ends the stream with that error,
/// since an empty room list would read to the panel as "the server has no rooms".
#[derive(Debug)]
pub struct MpscLiveKitRoomsStream {
    rx: tokio::sync::mpsc::UnboundedReceiver<Result<LiveKitRoomsEvent, Status>>,
}

impl Stream for MpscLiveKitRoomsStream {
    type Item = Result<LiveKitRoomsEvent, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

/// Stream adapter backed by an mpsc channel for [`HostStatsEvent`] server-streaming.
#[derive(Debug)]
pub struct MpscHostStatsStream {
    rx: tokio::sync::mpsc::UnboundedReceiver<HostStatsEvent>,
}

impl Stream for MpscHostStatsStream {
    type Item = Result<HostStatsEvent, Status>;

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

impl Unpin for MpscHostStatsStream {}

/// Stream adapter backed by an mpsc channel for [`WorktreeStatsEvent`] server-streaming. The first
/// event carries a full snapshot; each subsequent event carries one worktree's updated size row.
#[derive(Debug)]
pub struct MpscWorktreeStatsStream {
    rx: tokio::sync::mpsc::UnboundedReceiver<WorktreeStatsEvent>,
}

impl Stream for MpscWorktreeStatsStream {
    type Item = Result<WorktreeStatsEvent, Status>;

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

impl Unpin for MpscWorktreeStatsStream {}

/// Map the library disk-size lifecycle status to its wire enum.
fn proto_worktree_size_status(status: WorktreeSizeStatus) -> ProtoWorktreeSizeStatus {
    match status {
        WorktreeSizeStatus::None => ProtoWorktreeSizeStatus::None,
        WorktreeSizeStatus::Calculating => ProtoWorktreeSizeStatus::Calculating,
        WorktreeSizeStatus::Cached => ProtoWorktreeSizeStatus::Cached,
    }
}

/// Build a `WorktreeRow` from a worktree's branch/diff summary plus its current size state. The
/// size fields (`disk_bytes`, `size_status`, `size_calculated_at_unix_ms`) come from the
/// calculator; `disk_bytes`/timestamp are 0 until a size has been computed.
fn worktree_row_from_diff(
    diff: &WorktreeDiffRow,
    status: WorktreeSizeStatus,
    disk_bytes: Option<u64>,
    calculated_at_unix_ms: Option<i64>,
) -> WorktreeRow {
    WorktreeRow {
        path: diff.path.to_string_lossy().to_string(),
        branch_label: diff.branch_label.clone(),
        disk_bytes: disk_bytes.unwrap_or(0),
        changed_files: diff.changed_files,
        lines_added: diff.lines_added,
        lines_removed: diff.lines_removed,
        updated_at_unix_ms: calculated_at_unix_ms.unwrap_or(0),
        stale: false,
        size_status: proto_worktree_size_status(status) as i32,
        size_calculated_at_unix_ms: calculated_at_unix_ms.unwrap_or(0),
    }
}

/// Milliseconds since the Unix epoch, for agent-activity timestamps.
pub(crate) fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Live pub/sub hub for **agent activity** records, plus the per-session pending-call stack that
/// pairs a claude-cli `PreToolUse` (running) hook with its matching `PostToolUse` (terminal) hook.
///
/// Modeled on the per-session terminal-control broadcast ([`CliSessionManager::subscribe_control`]
/// / [`relay_control_events`]): [`subscribe`](AgentActivityHub::subscribe) hands out a
/// `broadcast::Receiver` for a session (creating the sender lazily) and
/// [`publish`](AgentActivityHub::publish) fans a record out to every current subscriber. The
/// durable `agent-activity.jsonl` log remains the source of truth — the hub only accelerates live
/// delivery, so publishing with no subscribers is a no-op.
#[derive(Default)]
pub struct AgentActivityHub {
    /// Per-session live broadcast; the sender is created lazily on first subscribe or publish.
    senders: StdMutex<
        std::collections::HashMap<
            String,
            tokio::sync::broadcast::Sender<tddy_core::agent_activity::AgentActivityRecord>,
        >,
    >,
    /// Per-session stack of in-flight `call_id`s awaiting their terminal (PostToolUse) row.
    pending: StdMutex<std::collections::HashMap<String, Vec<String>>>,
}

impl AgentActivityHub {
    /// Broadcast capacity per session. Sized so a burst of tool calls between a slow subscriber's
    /// polls rarely forces a `Lagged`; the relay tolerates `Lagged` regardless.
    const CHANNEL_CAPACITY: usize = 256;

    /// Subscribe to live records for `session_id`, creating the broadcast channel if absent.
    pub fn subscribe(
        &self,
        session_id: &str,
    ) -> tokio::sync::broadcast::Receiver<tddy_core::agent_activity::AgentActivityRecord> {
        let mut senders = self
            .senders
            .lock()
            .expect("agent activity hub mutex poisoned");
        let sender = senders
            .entry(session_id.to_string())
            .or_insert_with(|| tokio::sync::broadcast::channel(Self::CHANNEL_CAPACITY).0);
        sender.subscribe()
    }

    /// Publish a record to all live subscribers of `session_id`. A no-op when none are attached.
    pub fn publish(
        &self,
        session_id: &str,
        record: tddy_core::agent_activity::AgentActivityRecord,
    ) {
        let sender = {
            let senders = self
                .senders
                .lock()
                .expect("agent activity hub mutex poisoned");
            senders.get(session_id).cloned()
        };
        if let Some(sender) = sender {
            // Err = no live receivers; the durable log still holds the record, so ignore it.
            let _ = sender.send(record);
        }
    }

    /// Push an in-flight `call_id` onto the session's pending stack (a `PreToolUse` started a call).
    pub fn push_pending(&self, session_id: &str, call_id: &str) {
        let mut pending = self
            .pending
            .lock()
            .expect("agent activity hub mutex poisoned");
        pending
            .entry(session_id.to_string())
            .or_default()
            .push(call_id.to_string());
    }

    /// Pop the most-recent in-flight `call_id` for the session (its `PostToolUse` arrived). Returns
    /// `None` when no `PreToolUse` is outstanding, so the caller mints a fresh id instead.
    pub fn pop_pending(&self, session_id: &str) -> Option<String> {
        let mut pending = self
            .pending
            .lock()
            .expect("agent activity hub mutex poisoned");
        pending.get_mut(session_id).and_then(|stack| stack.pop())
    }
}

/// Relay task for `StreamSessionActivity`: forwards live agent-activity records for one session
/// (the broadcast is already session-scoped) from the hub into `tx` until the client disconnects.
async fn relay_agent_activity(
    mut broadcast_rx: tokio::sync::broadcast::Receiver<
        tddy_core::agent_activity::AgentActivityRecord,
    >,
    tx: tokio::sync::mpsc::UnboundedSender<ProtoAgentActivityRecord>,
) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match broadcast_rx.recv().await {
            Ok(record) => {
                if tx
                    .send(tddy_service::agent_activity_to_proto(record))
                    .is_err()
                {
                    break;
                }
            }
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
///
/// `Clone` is a shallow, shared clone: every mutable field is behind an `Arc`, so a clone talks to
/// the same session managers, registries and caches. The server-streaming handlers need it — they
/// hand the work to a `tokio::spawn`ed producer task, which must own a `'static` service.
#[derive(Clone)]
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
    /// Lazy, semaphore-bounded per-worktree disk-size calculator backing `StreamWorktreeStats` and
    /// `CalculateWorktreeSize`. Shares the stats cache root so persisted sizes survive restarts.
    worktree_size_calculator: Arc<WorktreeSizeCalculator>,
    claude_cli_manager: Arc<CliSessionManager>,
    /// Sandboxed claude-cli sessions (darwin Seatbelt).
    sandbox_manager: Arc<crate::sandbox_session::SandboxSessionManager>,
    /// Registry for Tasks created by tool invocations (every ExecuteTool call).
    task_registry: TaskRegistry,
    /// Optional idle-timeout tracker for relay mode — bumped on every RPC call.
    idle_tracker: Option<Arc<crate::relay_idle::IdleTimeoutTracker>>,
    /// Host machine stats provider (per-core CPU + project-dir disk) for the Host Stats Footer.
    host_stats: Arc<dyn HostStats>,
    /// Cadence for refreshing CPU on the `StreamHostStats` sampling loop (overridable for tests).
    host_cpu_interval: Duration,
    /// Cadence for refreshing disk on the `StreamHostStats` sampling loop (overridable for tests).
    host_disk_interval: Duration,
    /// Reader for the LiveKit server's rooms and their participants, behind `StreamLiveKitRooms`.
    room_roster: Arc<dyn RoomRoster>,
    /// Cadence at which a `StreamLiveKitRooms` subscription re-reads the roster (overridable for
    /// tests).
    room_poll_interval: Duration,
    /// Per-session demo VM state — keyed by session_id.
    demo_vm_state: Arc<tokio::sync::Mutex<std::collections::HashMap<String, DemoVmHandle>>>,
    /// Per-session reverse stdio RPC endpoint to a spawned tddy-coder child (grill-me), keyed by
    /// session_id. Hosts [`crate::host_session_service::HostSessionService`] so the coder can relay
    /// `spawn_conversation` back to the daemon over the pipe. Kept alive for the session's lifetime.
    session_stdio: Arc<tokio::sync::Mutex<std::collections::HashMap<String, SessionStdioEndpoint>>>,
    /// Live pub/sub hub for agent-activity records (StreamSessionActivity) plus the PreToolUse /
    /// PostToolUse pending-call pairing state. Shared with the sandbox tool handler so both the
    /// hook path and the in-jail tool path publish through the same channel.
    agent_activity_hub: Arc<AgentActivityHub>,
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
    session_rooms: Arc<crate::session_room::SessionRoomRegistry>,
    /// This daemon's model registry, whose assistants are selectable agents alongside the
    /// `allowed_agents` config entries. `None` means no registry is wired (a test fixture), in
    /// which case `ListAgents` reports the config entries alone.
    model_registry: Option<Arc<crate::model_registry::ModelRegistryStore>>,
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
    agent_conversations:
        Arc<tokio::sync::Mutex<std::collections::HashMap<String, AgentConversation>>>,
    /// Self-reference for handing out `Arc<ConnectionServiceImpl>` from a `&self` method. Set
    /// once (via [`Self::set_self_handle`]) right after the top-level `Arc::new` in `main.rs`;
    /// shared across `Clone`s because it is itself behind an `Arc`, so a clone tonic holds can
    /// still recover the original `Arc`. Used by the sandbox-IPC RPC bridge: a sandboxed session's
    /// `dial_and_bridge` builds a `DaemonRpcHandler` from `self_arc()` so the in-jail `tddy-tools`
    /// can reach the roster and conversation RPCs on this daemon over the `SessionChannel`.
    self_handle: Arc<std::sync::OnceLock<std::sync::Weak<ConnectionServiceImpl>>>,
}

/// One open conversation with a roster agent.
///
/// The two variants are what the main agent must not be able to tell apart: both answer
/// `{stopReason, content}`, and only the daemon deciding where the turn loop runs sees the
/// difference.
enum AgentConversation {
    /// The turn loop runs here, in this process.
    Local {
        session_id: String,
        agent_id: String,
        /// Shared rather than owned by the map, so a turn can be awaited on this lock alone with the
        /// map's lock released — a turn that pinned the map would block every cancel for its whole
        /// duration, including the cancel meant to interrupt it.
        session: Arc<tokio::sync::Mutex<Box<dyn tddy_discovery::subagent::SubagentSession>>>,
        /// Signalled when the conversation is closed. `notify_one` rather than `notify_waiters`, so
        /// a cancel that lands between the turn being spawned and its first await is still seen.
        closed: Arc<tokio::sync::Notify>,
    },
    /// The turn loop runs on `daemon_instance_id`; this daemon forwards to it.
    Remote {
        session_id: String,
        agent_id: String,
        daemon_instance_id: String,
    },
}

/// The clone an attach claimed for a remote agent.
///
/// `commissioned` is what makes a failed attach unwindable without taking a checkout away from an
/// agent that is still using it: two agents on one host share one clone, and only the attach that
/// minted it may delete it.
struct ClaimedAgentClone {
    codebase_session_id: String,
    commissioned: bool,
}

/// What one open conversation hands a prompt, taken out of the map so the map's lock can be
/// released before the turn is awaited.
enum PromptRouting {
    Local {
        session: Arc<tokio::sync::Mutex<Box<dyn tddy_discovery::subagent::SubagentSession>>>,
        closed: Arc<tokio::sync::Notify>,
    },
    Remote(String),
}

impl AgentConversation {
    /// Whether this conversation is with `agent_id` on `session_id`, whichever daemon runs its loop.
    fn is_with(&self, session_id: &str, agent_id: &str) -> bool {
        let (open_session, open_agent) = match self {
            AgentConversation::Local {
                session_id,
                agent_id,
                ..
            } => (session_id, agent_id),
            AgentConversation::Remote {
                session_id,
                agent_id,
                ..
            } => (session_id, agent_id),
        };
        open_session == session_id && open_agent == agent_id
    }
}

/// A live reverse stdio endpoint to one spawned tddy-coder session. Holding it keeps the pipe's
/// read/dispatch loop running; dropping it (on session teardown) ends the loop.
struct SessionStdioEndpoint {
    #[allow(dead_code)]
    client: Arc<tddy_stdio::StdioRpcClient>,
    #[allow(dead_code)]
    task: tokio::task::JoinHandle<()>,
}

impl ConnectionServiceImpl {
    /// Resolve the `tddy-tools` binary as a sibling of the configured tool (`tddy-coder`) path, so
    /// an installed deployment and a dev `target/debug` tree both find the co-located binary. Falls
    /// back to a bare `tddy-tools` (PATH lookup) when the tool path has no directory component.
    fn resolve_tddy_tools_path(&self) -> PathBuf {
        let base = self.config.default_tool_path();
        let base = Path::new(&base);
        match base.parent().filter(|p| !p.as_os_str().is_empty()) {
            Some(dir) => dir.join("tddy-tools"),
            None => PathBuf::from("tddy-tools"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: DaemonConfig,
        sessions_base_for_user: SessionsBaseResolver,
        tddy_data_dir: PathBuf,
        user_resolver: SessionUserResolver,
        spawn_client: Option<(spawn_worker::SpawnClient, i32)>,
        livekit_discovery: Option<LiveKitDiscoveryHandles>,
        telegram: Option<Arc<TelegramDaemonHooks>>,
        claude_cli_manager: Arc<CliSessionManager>,
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
        // Daemon-global cap of 2 concurrent size walks; shares the stats cache root so a fresh
        // calculator serves persisted sizes without re-walking.
        let worktree_size_calculator = Arc::new(WorktreeSizeCalculator::new(
            worktrees::projects_stats_cache_root(&tddy_data_dir),
            2,
        ));
        let task_registry = claude_cli_manager.task_registry();
        let demo_vm_state = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let session_stdio = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let host_stats: Arc<dyn HostStats> =
            Arc::new(SysinfoHostStats::new(resolve_default_project_dir(&config)));
        let room_roster = room_roster_from_config(config.livekit.as_ref());
        // Built here rather than inline below because the roster store reads it: an entry's
        // `clone_state` is the state of the checkout serving it, and two stores would let a roster
        // report READY for a clone nobody built.
        let session_agent_clones =
            Arc::new(crate::session_agent_clone::SessionAgentCloneStore::new());
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
            worktree_size_calculator,
            claude_cli_manager,
            sandbox_manager: Arc::new(crate::sandbox_session::SandboxSessionManager::new()),
            task_registry,
            idle_tracker: None,
            host_stats,
            host_cpu_interval: HOST_CPU_INTERVAL,
            host_disk_interval: HOST_DISK_INTERVAL,
            room_roster,
            room_poll_interval: LIVEKIT_ROOMS_POLL_INTERVAL,
            demo_vm_state,
            session_stdio,
            agent_activity_hub: Arc::new(AgentActivityHub::default()),
            github_token_store: None,
            staging_base_dir: crate::session_attachment_staging::default_staging_base_dir(),
            session_rooms: Arc::new(crate::session_room::SessionRoomRegistry::new()),
            model_registry: None,
            session_agent_rosters: Arc::new(
                crate::session_agent_roster::SessionAgentRosterStore::new(Arc::clone(
                    &session_agent_clones,
                )),
            ),
            session_agent_clones,
            hosted_agent_clones: Arc::new(crate::session_agent_clone::HostedAgentClones::new()),
            session_admissions: Arc::new(
                crate::session_admission_service::SessionAdmissionRegistry::new(),
            ),
            agent_conversations: Arc::new(
                tokio::sync::Mutex::new(std::collections::HashMap::new()),
            ),
            self_handle: Arc::new(std::sync::OnceLock::new()),
        }
    }

    /// Record the `Weak` to the top-level `Arc<ConnectionServiceImpl>` so a `&self` method can
    /// recover the `Arc` via [`Self::self_arc`]. Called once, right after `Arc::new`, in `main.rs`.
    /// Shared across `Clone`s (the field is an `Arc<OnceLock<…>>`), so a clone tonic holds still
    /// sees the same handle. Idempotent: a second call is a no-op, which is what tests want when
    /// they re-construct a service in the same process.
    pub fn set_self_handle(&self, handle: std::sync::Weak<ConnectionServiceImpl>) {
        let _ = self.self_handle.set(handle);
    }

    /// Recover the `Arc<ConnectionServiceImpl>` this `&self` belongs to. Panics if
    /// [`Self::set_self_handle`] was never called — which is a wiring bug, not a runtime condition:
    /// the daemon's `main.rs` sets it at startup, and tests that do not exercise the sandbox-IPC
    /// RPC bridge never call this.
    pub fn self_arc(&self) -> Arc<ConnectionServiceImpl> {
        self.self_handle
            .get()
            .and_then(|weak| weak.upgrade())
            .expect("ConnectionServiceImpl::self_arc called before set_self_handle")
    }

    /// Share this daemon's model registry (builder), so an assistant defined in it is listed by
    /// `ListAgents` as a selectable agent.
    pub fn with_model_registry(
        mut self,
        registry: Arc<crate::model_registry::ModelRegistryStore>,
    ) -> Self {
        self.model_registry = Some(registry);
        self
    }

    /// The per-session admission registry — shared with the `SessionAdmissionService` served on
    /// the common room, so the attach path records the first admit, the RPC refreshes it, and the
    /// detach/session-delete paths revoke against the same set (PRD § "What attach does" step 3).
    pub fn session_admissions(
        &self,
    ) -> Arc<crate::session_admission_service::SessionAdmissionRegistry> {
        Arc::clone(&self.session_admissions)
    }

    /// Whether this daemon currently hosts the session room for `session_id` — the
    /// `SessionAdmissionService`'s `session_exists` check, exposed so a test fixture wiring the
    /// admission service against this daemon's registry can build the same checker `main.rs` does
    /// without reaching into private fields.
    pub fn hosts_session(&self, session_id: &str) -> bool {
        self.session_rooms.contains(session_id)
    }

    /// The shared session-room registry — so a test fixture wiring `SessionAdmissionService`
    /// against this daemon can build the `session_exists` checker `main.rs` builds over the same
    /// registry the connection service updates when it opens and closes a session room.
    pub fn session_rooms(&self) -> Arc<crate::session_room::SessionRoomRegistry> {
        Arc::clone(&self.session_rooms)
    }

    /// The first admit — the facilitating daemon records the owning daemon in the admission registry
    /// and mints the scoped, short-TTL token it forwards along with the StartSession (PRD § "What
    /// attach does" step 3). The owning daemon joins `session-{session_id}` with this token and
    /// nothing else, then runs the re-admit loop against `AdmitOwningDaemon` before it expires.
    ///
    /// Returns `None` when this daemon cannot admit (LiveKit not configured), so a caller can skip
    /// the handshake and fall back to the owning daemon self-minting — never silently, but as a
    /// recorded deviation. `Some(token, url, room, ttl)` is what the caller forwards.
    fn mint_first_admission_token(
        &self,
        session_id: &str,
        owning_daemon_instance_id: &str,
    ) -> Option<(String, String, String, u64)> {
        use crate::livekit_peer_discovery::{
            daemon_rpc_identity, livekit_common_room_connect_strings,
        };
        use crate::session_admission_service::ADMISSION_TOKEN_TTL;
        use crate::session_room::session_room_name;
        use tddy_livekit::TokenGenerator;

        let (_common_room, url, api_key, api_secret) =
            livekit_common_room_connect_strings(&self.config).ok()?;
        self.session_admissions
            .admit(session_id, owning_daemon_instance_id);
        let room = session_room_name(session_id);
        let identity = daemon_rpc_identity(owning_daemon_instance_id);
        let token = TokenGenerator::new(
            api_key,
            api_secret,
            room.clone(),
            identity,
            ADMISSION_TOKEN_TTL,
        )
        .generate()
        .ok()?;
        log::info!(
            "provision_agent_clone: minted first admission token for daemon \
             {owning_daemon_instance_id} to session {session_id} (room {room}, ttl={}s)",
            ADMISSION_TOKEN_TTL.as_secs()
        );
        Some((token, url, room, ADMISSION_TOKEN_TTL.as_secs()))
    }

    /// Share the daemon's session-room registry (builder) with everything else that opens or
    /// closes rooms — Telegram's Delete path holds the same `Arc`. Without it this service keeps
    /// the private registry it was constructed with, which is right for a test fixture and wrong
    /// for a daemon, where a room opened here has to be closable from there.
    pub fn with_session_rooms(
        mut self,
        rooms: Arc<crate::session_room::SessionRoomRegistry>,
    ) -> Self {
        self.session_rooms = rooms;
        self
    }

    /// This daemon as the host of the rooms of the sessions it runs agents for.
    ///
    /// Handed to the agent-start path so the room is open *before* the agent process exists, which
    /// is what makes "the facilitating daemon is the first participant" a consequence of ordering
    /// rather than a race (PRD FR2).
    fn session_room_host(&self) -> DaemonSessionRoomHost {
        DaemonSessionRoomHost {
            config: self.config.clone(),
            instance_id: local_instance_id_for_config(&self.config),
            rooms: Arc::clone(&self.session_rooms),
            service: self.clone(),
        }
    }

    /// Substitute the pre-session attachment staging base (builder pattern) — lets a test point
    /// staging at a `TempDir` it owns and assert *where* staged bytes land, instead of sharing the
    /// process temp dir with every other test run.
    pub fn with_staging_base_dir(mut self, staging_base_dir: PathBuf) -> Self {
        self.staging_base_dir = staging_base_dir;
        self
    }

    /// Act on the operator's own GitHub credential for PR-status reads (builder). The store is the
    /// one the auth service writes to at login; without it, PR status reports itself unavailable.
    pub fn with_github_token_store(
        mut self,
        store: Arc<dyn tddy_github::token_store::GitHubTokenStore>,
    ) -> Self {
        self.github_token_store = Some(store);
        self
    }

    /// Shared agent-activity hub, so the sandbox tool path can publish through the same channel the
    /// `StreamSessionActivity` subscribers read.
    pub fn agent_activity_hub(&self) -> Arc<AgentActivityHub> {
        Arc::clone(&self.agent_activity_hub)
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

    /// Substitute the host machine stats provider (builder pattern) — lets tests inject a
    /// deterministic fake in place of the live `sysinfo`-backed provider.
    pub fn with_host_stats(mut self, host_stats: Arc<dyn HostStats>) -> Self {
        self.host_stats = host_stats;
        self
    }

    /// Substitute the per-worktree disk-size calculator (builder pattern) — lets tests inject a
    /// deterministic, instant sizer via [`WorktreeSizeCalculator::with_sizer`] in place of the live
    /// directory walk.
    pub fn with_worktree_size_calculator(
        mut self,
        calculator: Arc<WorktreeSizeCalculator>,
    ) -> Self {
        self.worktree_size_calculator = calculator;
        self
    }

    /// Override the `StreamHostStats` sampling cadence (builder pattern) — lets tests inject tiny
    /// intervals so cadence-driven refresh can be asserted deterministically without real-time waits.
    pub fn with_host_stats_intervals(mut self, cpu: Duration, disk: Duration) -> Self {
        self.host_cpu_interval = cpu;
        self.host_disk_interval = disk;
        self
    }

    /// Substitute the LiveKit rooms reader (builder pattern) — lets tests drive a scripted roster
    /// sequence in place of a live LiveKit server.
    pub fn with_room_roster(mut self, room_roster: Arc<dyn RoomRoster>) -> Self {
        self.room_roster = room_roster;
        self
    }

    /// Override the `StreamLiveKitRooms` poll cadence (builder pattern) — lets tests observe a
    /// change event without waiting the production three seconds for it.
    pub fn with_room_poll_interval(mut self, interval: Duration) -> Self {
        self.room_poll_interval = interval;
        self
    }

    /// Record RPC activity in the idle-timeout tracker, if one is attached.
    fn record_rpc_activity(&self) {
        if let Some(ref tracker) = self.idle_tracker {
            tracker.record_activity();
        }
    }

    /// Resolves the caller's per-user sessions base from a `session_token`, rejecting an invalid
    /// token before any filesystem access. Shared by the session-uploads RPCs (list/delete), which
    /// address files under `{sessions_base}/sessions/{session_id}/uploads/`.
    fn uploads_sessions_base(&self, session_token: &str) -> Result<PathBuf, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))
    }

    fn maybe_spawn_telegram_observer(&self, session_id: &str, grpc_port: u16) {
        if let Some(ref tg) = self.telegram {
            tg.spawn_presenter_observer_task(session_id, grpc_port);
        }
    }

    /// PR status for one branch, resolved with the calling operator's own GitHub credential.
    ///
    /// `repo_root` is `None` when no file in the session directory records a checkout (see
    /// [`tddy_core::repo_root_for_session`]) — an unknown repository, not a repository without PRs.
    ///
    /// Never fails: a lookup that cannot be performed degrades this one field to *unavailable* (D8),
    /// and stub/demo authentication resolves to an empty result (D12) — so the enclosing RPC keeps
    /// returning its other legs instead of collapsing into an error the web discards wholesale.
    async fn pr_status_for_caller(
        &self,
        github_login: &str,
        repo_root: Option<&std::path::Path>,
        branch: &str,
    ) -> tddy_service::proto::connection::PrStatusView {
        use crate::github_pr_credentials::{pr_lookup_for_caller, PrLookup};
        use tddy_service::proto::connection::PrStatusView;
        use tddy_workflow_recipes::orchestrate_pr_stack::github::PrLookupOutcome;

        // Both ways of failing to name a GitHub repository leave the lookup un-performable, so they
        // are *unavailable* with a reason — reporting `exists = false` would claim the branch has no
        // PR when in fact nothing was ever asked (D8).
        let Some(repo_root) = repo_root else {
            return pr_status_unavailable(
                branch,
                "no checkout is recorded for this session, so its GitHub repository is unknown"
                    .to_string(),
            );
        };
        let Some(owner_repo) = owner_repo_from_repo_root(repo_root) else {
            return pr_status_unavailable(
                branch,
                format!(
                    "no GitHub repository could be resolved from the origin remote of {}",
                    repo_root.display()
                ),
            );
        };
        let stub_mode = self
            .config
            .github
            .as_ref()
            .and_then(|g| g.stub)
            .unwrap_or(false);
        let stored = self
            .github_token_store
            .as_ref()
            .and_then(|store| store.get(github_login));
        let token = match pr_lookup_for_caller(stub_mode, stored.as_deref()) {
            PrLookup::Empty => return PrStatusView::default(),
            PrLookup::Unavailable(reason) => return pr_status_unavailable(branch, reason),
            PrLookup::Perform(token) => token,
        };

        let head_branch = branch.to_string();
        let outcome = tokio::task::spawn_blocking(move || {
            use tddy_workflow_recipes::orchestrate_pr_stack::github::GithubPrApi;
            tddy_workflow_recipes::orchestrate_pr_stack::github::RealGithubPrApi::with_token(
                owner_repo, token,
            )
            .get_pr_by_head(&head_branch)
        })
        .await;

        match outcome {
            Ok(PrLookupOutcome::Found(pr)) => PrStatusView {
                exists: true,
                number: pr.number,
                url: pr.url,
                state: pr_state_label(pr.state).to_string(),
                unavailable: false,
                unavailable_reason: String::new(),
            },
            Ok(PrLookupOutcome::NotFound) => PrStatusView::default(),
            Ok(PrLookupOutcome::Unavailable(reason)) => pr_status_unavailable(branch, reason),
            Err(join_error) => pr_status_unavailable(
                branch,
                format!("the PR lookup did not complete: {join_error}"),
            ),
        }
    }

    /// How the branch stands against the base the caller named, for `QueryBranch`'s fifth leg.
    ///
    /// Never fails, exactly like the session, worktree, remote and PR legs beside it: an unnamed
    /// base, an unknown checkout, a probe that could not run and a probe that ran out of time all
    /// arrive as `unavailable` carrying a reason. A comparison that could not be made is byte-
    /// identical to a healthy one on every other field, so the discriminator is the only thing that
    /// keeps "could not tell" from rendering as "clean" (PRD D27).
    ///
    /// An unnamed base is reported unavailable rather than substituted with the project default
    /// (D29): this is a display, and the number beside a row must describe the same base the row's
    /// own base line shows.
    async fn base_sync_leg(
        &self,
        repo_root: Option<&std::path::Path>,
        branch: &str,
        base_branch: &str,
    ) -> Option<tddy_service::proto::connection::BranchBaseSync> {
        if base_branch.is_empty() {
            return Some(base_sync_unavailable(
                "",
                "no base branch was named for this branch, so there is nothing to compare it \
                 against",
            ));
        }
        let Some(repo_root) = repo_root else {
            return Some(base_sync_unavailable(
                base_branch,
                "no checkout is recorded for this session, so its repository could not be resolved",
            ));
        };

        let probe_root = repo_root.to_path_buf();
        let probe_branch = branch.to_string();
        let probe_base = base_branch.to_string();
        let probed = spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "QueryBranch: compare branch against base",
            move || {
                Ok(base_sync_through_cache(
                    &probe_root,
                    &probe_branch,
                    &probe_base,
                ))
            },
        )
        .await;

        Some(match probed {
            Ok(Ok(sync)) => base_sync_view(sync),
            Ok(Err(reason)) => base_sync_unavailable(base_branch, &reason),
            // A timeout degrades this leg; it must not take the other four with it.
            Err(status) => base_sync_unavailable(
                base_branch,
                &format!("the comparison did not complete: {}", status.message()),
            ),
        })
    }

    fn resolve_chain_base_ref_status(
        sessions_base: &std::path::Path,
        stack_parent: Option<&str>,
        repo_root: &std::path::Path,
        new_branch_name: &str,
    ) -> Result<Option<String>, Status> {
        tddy_core::resolve_chain_base_ref(sessions_base, stack_parent, repo_root, new_branch_name)
            .map_err(Status::failed_precondition)
    }

    /// Record on a pr-stack orchestrator's planned node the branch a child spawn just created, plus
    /// the child session as the fallback route back to that branch.
    ///
    /// The spawn paths already write the *reverse* link (`orchestrator_session_id` in the child's
    /// changeset). Without this forward link the node owns no branch, and the stack wedges:
    /// [`tddy_core::changeset::Stack::base_ref_for_spawn`] refuses every descendant ("non-merged
    /// parent … has no branch to base onto yet"), [`StackChildSpawnHandler`]'s duplicate-spawn guard
    /// never trips, and the orchestrator dashboard shows no child state or PR for a running child.
    ///
    /// Called once the child's branch exists (right after worktree setup) — that is precisely the
    /// condition `base_ref_for_spawn` gates descendants on. A new session claiming a branch a node
    /// already owns repoints the fallback to it (last writer wins): the branch is what the stack is
    /// built on, and sessions on it come and go (restart, re-attach) without changing that.
    fn link_stack_node_to_spawned_branch(
        sessions_base: &std::path::Path,
        stack_parent: Option<&str>,
        new_branch_name: &str,
        child_session_id: &str,
    ) -> Result<(), Status> {
        let Some(sp) = stack_parent else {
            return Ok(());
        };
        let Some((parent_dir, _stack, node_id)) =
            tddy_core::pr_stack_node_for_spawn(sessions_base, sp, new_branch_name)
        else {
            return Ok(());
        };
        tddy_core::changeset::link_stack_node_to_child_session(
            &parent_dir,
            &node_id,
            child_session_id,
            Some(new_branch_name.trim().to_string()),
        )
        .map_err(|e| {
            Status::internal(format!(
                "failed to link stack node '{node_id}' to child session {child_session_id}: {e}"
            ))
        })?;
        log::info!(
            target: "tddy_daemon::connection_service",
            "recorded branch '{}' on pr-stack node '{}' of orchestrator {} (child session {})",
            new_branch_name.trim(),
            node_id,
            sp,
            child_session_id
        );
        Ok(())
    }

    /// Handle `StartSession` for `session_type = "claude-cli"` sessions.
    ///
    /// Requires a valid, registered project. Creates a real git worktree under the project's
    /// main repo (via `tddy_core::setup_worktree_for_session_with_optional_chain_base`), then
    /// spawns the `claude` binary in a PTY.
    ///
    /// `initial_prompt` — when non-empty, passed as a positional argument to `claude` so it
    /// receives the first user turn on startup (e.g. `claude "build feature X"`).
    /// Resolve the goal a managed session should resume at: the goal persisted in `changeset.yaml`,
    /// falling back to the recipe's start goal when no meaningful state is recorded yet (empty or the
    /// default `Init`). Managed sessions persist a valid goal id on every committed transition.
    fn managed_resume_goal(
        session_dir: &Path,
        recipe: &Arc<dyn tddy_core::backend::WorkflowRecipe>,
    ) -> tddy_core::backend::GoalId {
        let persisted = tddy_core::read_changeset(session_dir)
            .ok()
            .map(|cs| cs.state.current.into_inner())
            .unwrap_or_default();
        let p = persisted.trim();
        if p.is_empty() || p == "Init" {
            recipe.start_goal()
        } else {
            tddy_core::backend::GoalId::new(p)
        }
    }

    /// Build the managed-workflow wiring for a claude-cli session and return the launch inputs: the
    /// [`ManagedWorkflow`](crate::session_toolcall::ManagedWorkflow) (its listener must be kept alive
    /// for the session's lifetime), the orchestration-prompt file path to append to claude's system
    /// prompt, and the per-session env (`TDDY_SOCKET` + a `PATH` that resolves `tddy-tools`) for the
    /// process that runs `tddy-tools transition`.
    ///
    /// `prompt_dir` is where the prompt file is written: the session dir for a non-sandboxed session,
    /// the jail-visible context dir for a sandboxed one. `resume_at` selects the controller's initial
    /// goal — `None` for a new session (recipe start goal), `Some` to resume an existing one.
    #[allow(clippy::too_many_arguments)]
    fn prepare_managed_workflow(
        &self,
        session_id: &str,
        recipe: Arc<dyn tddy_core::backend::WorkflowRecipe>,
        session_dir: &Path,
        worktree_path: &Path,
        prompt_dir: &Path,
        tddy_tools_path: &str,
        resume_at: Option<tddy_core::backend::GoalId>,
        conversation_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>>,
    ) -> Result<ManagedLaunch, Status> {
        prepare_managed_workflow_inner(
            &self.tddy_data_dir,
            session_id,
            recipe,
            session_dir,
            worktree_path,
            prompt_dir,
            tddy_tools_path,
            resume_at,
            None,
            conversation_spawn_handler,
        )
    }

    /// The conflict a `StartSession` request must be answered with instead of creating anything, or
    /// `None` when creation may proceed.
    ///
    /// Fires only for an explicit `new_branch_from_base` with a non-empty `new_branch_name` and
    /// `on_branch_conflict = "reject"`:
    /// - a generated branch name (`claude-cli/<short-id>`, `workspace/<short-id>`) is derived from the
    ///   session uuid and cannot collide, so the empty intent is never checked;
    /// - `work_on_selected_branch` is the intent that deliberately joins an owned branch;
    /// - an empty `on_branch_conflict` keeps the suffixing behaviour every existing caller relies on.
    ///
    /// The project is resolved only once a conflict is established, so a request that may proceed
    /// keeps whatever error the session-type dispatch would have produced for its project.
    ///
    /// See docs/ft/daemon/session-branch-conflict.md.
    async fn owned_branch_conflict(
        &self,
        os_user: &str,
        req: &StartSessionRequest,
    ) -> Result<Option<BranchConflict>, Status> {
        use tddy_service::proto::connection::BranchSession;

        if req.on_branch_conflict.trim() != "reject"
            || req.branch_worktree_intent.trim() != "new_branch_from_base"
        {
            return Ok(None);
        }
        let branch = req.new_branch_name.trim().to_string();
        if branch.is_empty() {
            return Ok(None);
        }

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let branch_for_scan = branch.clone();
        let owner = spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "StartSession: scan sessions by branch",
            move || {
                crate::branch_owner::find_session_owning_branch(&sessions_base, &branch_for_scan)
            },
        )
        .await?;
        let Some(owner) = owner else {
            return Ok(None);
        };

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, req.project_id.trim())
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        let branch_for_suggestion = branch.clone();
        let suggested_branch_name = spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "StartSession: first free suffixed branch name",
            move || {
                Ok(tddy_core::worktree::first_free_suffixed_branch_name(
                    &repo_root,
                    &branch_for_suggestion,
                ))
            },
        )
        .await?;

        Ok(Some(BranchConflict {
            branch,
            owner: Some(BranchSession {
                exists: true,
                session_id: owner.session_id,
                is_active: owner.is_active,
                status: owner.status,
            }),
            suggested_branch_name,
        }))
    }

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
        dangerously_skip_permissions: bool,
        stack_parent: Option<&str>,
        // When `Some`, the session is launched workflow-aware: the recipe's orchestration prompt is
        // injected and its `transition` tool advances a per-session `WorkflowController`.
        managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>>,
        // When true, index the worktree before launch and expose the `SemanticSearch` tool.
        semantic_index: bool,
        // When true (new_branch_from_base only), push the new branch to origin at session start.
        create_remote_branch: bool,
    ) -> Result<Response<StartSessionResponse>, Status> {
        // A pr-stack orchestrator gets a child-spawn handler bound to its toolcall listener so the
        // agent's `pr_spawn_child` relay can materialize planned nodes into child sessions.
        let child_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ChildSpawnHandler>> =
            if managed_recipe
                .as_ref()
                .is_some_and(|r| r.name() == "pr-stack")
            {
                let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
                Some(Arc::new(StackChildSpawnHandler {
                    room_host: Arc::new(self.session_room_host()),
                    config: self.config.clone(),
                    tddy_data_dir: self.tddy_data_dir.clone(),
                    claude_cli_manager: Arc::clone(&self.claude_cli_manager),
                    os_user: os_user.to_string(),
                    project_id: project_id.to_string(),
                    sessions_base: sessions_base.clone(),
                    orchestrator_session_id: session_id.to_string(),
                    orchestrator_session_dir: session_dir,
                }))
            } else {
                None
            };
        // A grill-me session instead gets a conversation-spawn handler so the agent's
        // `spawn_conversation` relay can start a fresh implementation conversation.
        let conversation_spawn_handler = managed_recipe.as_ref().and_then(|recipe| {
            self.conversation_spawn_handler_for(
                recipe,
                os_user,
                session_id,
                project_id,
                &sessions_base,
                &sessions_base.join(SESSIONS_SUBDIR).join(session_id),
            )
        });
        spawn_claude_cli_session_inner(
            &self.config,
            &self.tddy_data_dir,
            &self.claude_cli_manager,
            os_user,
            session_id,
            sessions_base,
            model,
            project_id,
            branch_worktree_intent,
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
            initial_prompt,
            permission_mode,
            dangerously_skip_permissions,
            stack_parent,
            managed_recipe,
            child_spawn_handler,
            conversation_spawn_handler,
            semantic_index,
            create_remote_branch,
            &self.task_registry,
            &self.session_room_host(),
        )
        .await
    }

    /// Build the per-session [`ConversationSpawnHandler`] for a managed session when its recipe
    /// enables conversation spawning (grill-me). Returns `None` for recipes that don't (a plain TDD
    /// session, or a PR-stack orchestrator which uses `spawn-child` instead), so `spawn_conversation`
    /// is rejected there rather than silently spawning.
    fn conversation_spawn_handler_for(
        &self,
        recipe: &Arc<dyn tddy_core::backend::WorkflowRecipe>,
        os_user: &str,
        session_id: &str,
        project_id: &str,
        sessions_base: &Path,
        orchestrator_session_dir: &Path,
    ) -> Option<Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>> {
        if !recipe_enables_conversation_spawn(recipe.name()) {
            return None;
        }
        Some(Arc::new(GrillMeConversationSpawnHandler {
            room_host: Arc::new(self.session_room_host()),
            config: self.config.clone(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            claude_cli_manager: Arc::clone(&self.claude_cli_manager),
            os_user: os_user.to_string(),
            project_id: project_id.to_string(),
            sessions_base: sessions_base.to_path_buf(),
            orchestrator_session_id: session_id.to_string(),
            model_override: None,
            orchestrator_session_dir: orchestrator_session_dir.to_path_buf(),
        }))
    }

    /// Bind a per-session unix socket hosting
    /// [`HostSessionService`](crate::host_session_service::HostSessionService) and return its path,
    /// to be passed to the spawned grill-me coder as `--host-session-socket`. The coder connects and
    /// relays `spawn_conversation` back over it. The orchestrator context (this session) is baked
    /// into the handler, and the path is unique per session and handed only to that session's coder,
    /// so a call arriving here is unambiguously that session — no auth token or caller id needed.
    ///
    /// A socket **path** (unlike the child's stdio fds) crosses the forked `spawn_worker` boundary as
    /// a plain string, so this works for both the worker-spawned and direct spawn paths. Binding
    /// happens before the spawn, so the coder's later `connect()` finds the listener ready.
    async fn spawn_host_session_socket(
        &self,
        session_id: &str,
        os_user: &str,
        project_id: &str,
        model: Option<String>,
    ) -> Option<String> {
        let path = std::env::temp_dir().join(format!("tddy-host-{session_id}.sock"));
        let _ = std::fs::remove_file(&path); // clear any stale socket from a prior run
        let listener = match tokio::net::UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                log::warn!("spawn_host_session_socket({session_id}): bind {path:?}: {e}");
                return None;
            }
        };
        // Dev runs the coder as the same OS user; loosen perms so a cross-user child can still
        // connect. TODO(stdio-relay): tighten perms / socket ownership for cross-user production.
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777));

        let sessions_base = self.tddy_data_dir.clone();
        let orchestrator_session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        let handler = Arc::new(GrillMeConversationSpawnHandler {
            room_host: Arc::new(self.session_room_host()),
            config: self.config.clone(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            claude_cli_manager: Arc::clone(&self.claude_cli_manager),
            os_user: os_user.to_string(),
            project_id: project_id.to_string(),
            sessions_base,
            orchestrator_session_id: session_id.to_string(),
            orchestrator_session_dir,
            model_override: model,
        });
        let service = crate::host_session_service::HostSessionService::new(handler);
        let session_stdio = Arc::clone(&self.session_stdio);
        let sid = session_id.to_string();
        // Accept the coder's single connection, then run the reverse RPC endpoint over it.
        tokio::spawn(async move {
            let stream = match listener.accept().await {
                Ok((stream, _addr)) => stream,
                Err(e) => {
                    log::warn!("spawn_host_session_socket({sid}): accept: {e}");
                    return;
                }
            };
            let (reader, writer) = tokio::io::split(stream);
            let (client, endpoint) =
                tddy_stdio::StdioEndpoint::from_duplex(reader, writer, service);
            let task = tokio::spawn(endpoint.run());
            session_stdio
                .lock()
                .await
                .insert(sid.clone(), SessionStdioEndpoint { client, task });
            log::info!("spawn_host_session_socket({sid}): reverse endpoint connected + ready");
        });
        log::info!("spawn_host_session_socket({session_id}): listening at {path:?}");
        Some(path.to_string_lossy().into_owned())
    }
}

/// Write `.claude/settings.local.json` into `cwd` — the directory `claude` will run in — so Claude
/// Code wires this session's lifecycle hooks on startup.
///
/// Warn-and-continue: a session without hooks reports no status, which is worse than a session that
/// never started only if the operator cannot see it at all, and it still can.
fn write_claude_hooks_settings(cwd: &Path, params: &tddy_core::HookCommandParams<'_>) {
    let settings = tddy_core::build_claude_hooks_settings(params);
    let claude_dir = cwd.join(".claude");
    if let Err(e) = std::fs::create_dir_all(&claude_dir).and_then(|_| {
        serde_json::to_string_pretty(&settings)
            .map_err(|e| std::io::Error::other(e.to_string()))
            .and_then(|json| {
                tddy_core::atomic_file::write_atomic(&claude_dir.join("settings.local.json"), json)
            })
    }) {
        log::warn!(
            "session {}: failed to write .claude/settings.local.json — hooks will not fire: {e}",
            params.session_id
        );
    }
}

/// The web port a hook URL assumes when `listen.web_port` is unset. `startup` refuses to serve
/// without that setting, so this only covers a config the daemon would not have started from — but
/// building the URL is not the place to discover it.
const DEFAULT_WEB_PORT: u16 = 8899;

/// Where a hook command reaches this daemon when nothing is configured: its own web listener on
/// loopback.
///
/// The port default is here and nowhere else — a hook posting to the wrong port fails silently from
/// the operator's side, and three copies of `8899` is three chances for one of them to fall behind a
/// changed default.
pub fn local_daemon_hook_url(config: &DaemonConfig) -> String {
    format!(
        "http://127.0.0.1:{}",
        config.listen.web_port.unwrap_or(DEFAULT_WEB_PORT)
    )
}

/// Externally-reachable HTTP base URL peer daemons use to reach this daemon's Connect-HTTP surface
/// (today: `auth.LiveKitTokenService/MintLiveKitToken`, used by `tddy-remote-git-repo` to mint the
/// common-room LiveKit token before driving `remote_git.RemoteGitService/Serve`).
///
/// Explicit `listen.advertise_url` wins; otherwise the loopback URL derived from the web port —
/// the same default `claude_hook_daemon_url` falls back to, and for the same reason: a daemon that
/// never configured an external URL is one a peer on another host cannot reach, but one a peer on
/// the same host (and every test) can. The facilitating daemon publishes this in
/// `AgentClonePlacement.facilitating_daemon_url` so an owning daemon that has never seen the
/// project can clone it (PRD AC37).
pub fn advertise_daemon_url(config: &DaemonConfig) -> String {
    config
        .listen
        .advertise_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| local_daemon_hook_url(config))
}

/// Base URL a claude-cli session's hook commands call `ReportSessionStatus` on: the configured
/// `claude_cli.daemon_url`, else this daemon's own web port.
pub fn claude_hook_daemon_url(config: &DaemonConfig) -> String {
    config
        .claude_cli
        .as_ref()
        .and_then(|c| c.daemon_url.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| local_daemon_hook_url(config))
}

/// Resolve the `claude` binary for the interactive (non-sandboxed) StartSession path.
///
/// Delegates to [`crate::config::resolve_claude_binary_path`] so the interactive and sandboxed
/// spawn paths never diverge on which `claude` they pick (explicit config path honored; bare name
/// auto-resolved to a real host install).
pub fn resolve_start_session_claude_binary(config: &DaemonConfig) -> String {
    crate::config::resolve_claude_binary_path(config)
}

/// The branch a spawn actually operates on: the branch it creates, or — under
/// `work_on_selected_branch` — the existing branch it resumes.
///
/// A PR-stack node's link is keyed on this rather than on `new_branch_name`, which is **empty** for a
/// resume. Recovering a planned PR whose child session was deleted means resuming the branch the node
/// already owns (it exists, is pushed, has a worktree), and without the effective branch
/// `pr_stack_node_for_spawn` matches nothing: the node would never re-link, so the row would stay
/// recovered and every click would spawn another unlinked session.
///
/// A blank intent defaults to `new_branch_from_base` (`StartSessionRequest.branch_worktree_intent`),
/// and a resume ignores any leftover `new_branch_name` the dialog carried — keying on a branch the
/// spawn never touches would link the node to the wrong branch.
///
/// A resumed branch is reduced to its local name: the dialog's picker is fed by
/// `ListProjectBranches`, which offers remote-tracking names (`<remote>/<branch>`), while a stack
/// node records the local one. Keying on the prefixed form matches no node, which is the same
/// silent non-link this function exists to prevent. The `remote` argument is the project's resolved
/// default remote so a non-`origin` prefix is stripped correctly.
#[must_use]
pub fn effective_spawn_branch<'a>(
    branch_worktree_intent: &str,
    new_branch_name: &'a str,
    selected_branch_to_work_on: &'a str,
    remote: &str,
) -> &'a str {
    match branch_worktree_intent.trim() {
        "work_on_selected_branch" => {
            tddy_core::worktree::local_branch_name_for_remote(selected_branch_to_work_on, remote)
        }
        _ => new_branch_name.trim(),
    }
}

/// Resolves the default remote name for a registered project, degrading to an empty string when the
/// resolver itself errors (e.g. unreadable `projects.yaml`) so a list RPC never fails on a single
/// bad row. The resolver already falls back to `origin` as the last resort, so the empty case is the
/// rare "registry unreadable" path — clients apply their own `origin` fallback then.
fn resolve_default_remote_or_empty(
    projects_dir: &Path,
    project_id: &str,
    repo_root: &Path,
) -> String {
    project_storage::effective_remote_name_for_project(projects_dir, project_id, repo_root)
        .unwrap_or_default()
}

/// Builds a proto [`ProjectEntry`] from a stored [`project_storage::ProjectData`] plus the resolved
/// `default_remote`. Centralizing the mapping keeps every response (ListProjects, CreateProject,
/// AddProjectToHost, SetProjectDefaultBranch) consistent as fields are added.
fn project_entry_from(
    p: &project_storage::ProjectData,
    daemon_instance_id: String,
    default_remote: String,
) -> ProtoProjectEntry {
    ProtoProjectEntry {
        project_id: p.project_id.clone(),
        name: p.name.clone(),
        git_url: p.git_url.clone(),
        main_repo_path: p.main_repo_path.clone(),
        daemon_instance_id,
        main_branch_ref: p.main_branch_ref.clone().unwrap_or_default(),
        default_remote,
    }
}

/// The repoint target a client may act on: `Ok(None)` for "no target named", `Ok(Some(target))`
/// for an accepted one, `Err(reason)` for a target the daemon refuses.
///
/// `RepointPlannedPrRequest.target_base_branch` is applied by `repoint_planned_pr_node` as a
/// **retain** rule — the parents that own that branch stay and the rest are dropped — so a target
/// no parent owns *is* the instruction to detach the node onto the default branch. Validation is
/// therefore not politeness: a stale label, a typo, or a client that has drifted from the daemon's
/// view of the repo would each read as "detach this node" and silently rewrite the plan. An
/// accepted target must name either the resolved default branch or one of the node's parents'
/// branches; nothing else is a meaningful thing to be based onto.
///
/// An empty or whitespace-only target is not a rejection: it names no target at all and selects the
/// original drop-merged-parents rule (`None`).
///
/// The default branch is compared with the remote prefix stripped from both sides.
/// `tddy_core::resolve_default_integration_base_ref` returns a remote-tracking ref
/// (`<remote>/<branch>`), while a node's `branch` and a GitHub PR base are plain names, so the label
/// a client renders can legitimately carry either form. The remote is parsed off `default_branch`
/// (the segment before its first `/`) so a non-`origin` default is normalized correctly. The
/// accepted value returned is the caller's own trimmed input, not the normalized form, so the
/// recipe matches parent branches as recorded.
pub fn validate_repoint_target(
    target_base_branch: &str,
    default_branch: &str,
    parent_branches: &[&str],
) -> Result<Option<String>, String> {
    let target = target_base_branch.trim();
    if target.is_empty() {
        return Ok(None);
    }

    let remote = default_branch
        .split_once('/')
        .map(|(r, _)| r)
        .unwrap_or("origin");
    let names_default = tddy_core::worktree::local_branch_name_for_remote(target, remote)
        == tddy_core::worktree::local_branch_name_for_remote(default_branch, remote);
    let names_parent = parent_branches.contains(&target);

    if names_default || names_parent {
        Ok(Some(target.to_string()))
    } else {
        Err(format!(
            "target_base_branch '{target}' names neither the default branch '{default_branch}' nor any parent's branch"
        ))
    }
}

/// Resolve the `claude` binary for a ResumeSession relaunch through the same host resolver as
/// StartSession, so an explicitly configured path is honored and a bare name is resolved to a host
/// path instead of being spawned against the daemon's minimal systemd PATH.
pub fn resolve_resume_session_claude_binary(config: &DaemonConfig) -> String {
    crate::config::resolve_claude_binary_path(config)
}

/// The daemon's own [`crate::session_room::SessionRoomHost`].
///
/// Holds a clone of the service it will serve inside the room — the same `ConnectionService` it
/// answers on in the common room, so a participant reaches every file-access method without a
/// second connection anywhere (PRD FR3). That makes a reference cycle with the registry, which is
/// deliberate and broken by `close`: see `SessionRoomRegistry`.
struct DaemonSessionRoomHost {
    config: DaemonConfig,
    instance_id: String,
    rooms: Arc<crate::session_room::SessionRoomRegistry>,
    service: ConnectionServiceImpl,
}

/// The daemon measuring a checkout that lives on one of its peers.
///
/// Routed through its own `GetWorktreeSnapshot` handler rather than a bespoke client, so a remote
/// measurement takes exactly the path a caller's would — including the peer routing and the
/// blocking-pool budget.
#[async_trait::async_trait]
impl crate::session_room::RemoteSnapshotSource for ConnectionServiceImpl {
    async fn snapshot(
        &self,
        session_token: &str,
        codebase_session_id: &str,
        codebase_instance_id: &str,
    ) -> Result<crate::session_room::WorktreeSnapshot, Status> {
        let answered = ConnectionServiceTrait::get_worktree_snapshot(
            self,
            Request::new(GetWorktreeSnapshotRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session_id.to_string(),
                daemon_instance_id: codebase_instance_id.to_string(),
            }),
        )
        .await?
        .into_inner();
        Ok(crate::session_room::WorktreeSnapshot {
            head_commit: answered.head_commit,
            branch: answered.branch,
            changed_paths: answered.changed_paths,
            changed_files: answered.changed_files,
            lines_added: answered.lines_added,
            lines_removed: answered.lines_removed,
            untracked_files: answered.untracked_files,
            // FIXME(session-worktree-sync): a SPLIT session's snapshot arrives over
            // GetWorktreeSnapshot, whose response carries no tree — so the facilitating daemon
            // cannot diff a checkout it does not hold. Closing this means a `wip_tree` field on
            // GetWorktreeSnapshotResponse and the codebase daemon writing it. Until then a split
            // session syncs committed history only, and says so rather than mirroring silently
            // stale content. See docs/dev/TODO.md.
            wip_tree: String::new(),
        })
    }
}

#[async_trait::async_trait]
impl crate::session_room::SessionRoomHost for DaemonSessionRoomHost {
    async fn open_for(
        &self,
        session_id: &str,
        worktree_root: &Path,
        session_dir: &Path,
    ) -> Result<Option<crate::session_room::OpenedSessionRoom>, Status> {
        self.rooms
            .open(
                &crate::session_room::DaemonRoomHosting {
                    config: &self.config,
                    instance_id: &self.instance_id,
                    rooms: &self.rooms,
                }
                .for_worktree(session_id, worktree_root, session_dir),
                tddy_service::ConnectionServiceServer::new(self.service.clone()),
            )
            .await
    }
}

/// Open the session room of a session whose agent this daemon is about to spawn, and say which way
/// it went.
///
/// Every spawn path that starts an agent against a checkout this daemon holds goes through here —
/// claude-cli and cursor-cli, sandboxed or not. The room belongs to the daemon **running the
/// agent** (`docs/ft/daemon/session-room.md` § Roles), and on all four of those paths that daemon
/// is this one: it resolved the worktree, it holds the session directory, and it is about to fork
/// the agent. A `workspace` session is the one session type deliberately left out — it has no
/// agent, so no facilitating daemon and no room; see `crate::workspace_session`.
///
/// Before the agent exists, not after: the room's first-participant property is a consequence of
/// this `await` completing while the only thing that could join is still unspawned (PRD FR2). A
/// failure here fails the start — the agent's tool transport is minted for this room, so a session
/// started without it is a session whose agent has nowhere to ask for its files. A daemon with no
/// `livekit:` credentials at all is not a failure but a `None`: the room is an addition to a
/// session, never a prerequisite for one, and such a daemon starts sessions exactly as it did
/// before rooms existed.
///
/// Deliberately not returned in `StartSessionResponse.livekit_room`: that field names the
/// session's *terminal* room, which the browser attaches to, and the two are different rooms with
/// different participants. A caller that wants this one derives it from the session id through
/// `session_room_name`, which is how the agent's own wiring gets it too.
pub(crate) async fn open_session_room_before_spawning_agent(
    room_host: &dyn crate::session_room::SessionRoomHost,
    session_type: &str,
    session_id: &str,
    worktree_path: &Path,
    session_dir: &Path,
) -> Result<(), Status> {
    match room_host
        .open_for(session_id, worktree_path, session_dir)
        .await?
    {
        Some(room) => log::info!(
            "{session_type} session {session_id} facilitated in {} as {}",
            room.room,
            room.server_identity
        ),
        None => log::debug!(
            "{session_type} session {session_id} runs without a session room (LiveKit not configured)"
        ),
    }
    Ok(())
}

/// The exec-catalog names of the tools a def's own loop may call — the spelling the wire, the
/// roster and `execute_tool`'s dispatch all use, rather than the `UPPERCASE` YAML spelling.
fn def_tool_names(def: &tddy_discovery::agent_def::SpecializedAgentDef) -> Vec<String> {
    def.tools
        .iter()
        .map(|t| t.catalog_name().to_string())
        .collect()
}

/// One resolved def as the `ListSubagents` row a picker attaches from.
fn subagent_info(
    def: &tddy_discovery::agent_def::SpecializedAgentDef,
    daemon_instance_id: &str,
) -> Result<SubagentInfo, tddy_core::AgentIdError> {
    Ok(SubagentInfo {
        agent_id: qualified_agent_id(&def.name, daemon_instance_id)?,
        name: def.name.clone(),
        label: def
            .label
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| def.name.clone()),
        model: def.model.clone(),
        daemon_instance_id: daemon_instance_id.to_string(),
        replaces: tddy_discovery::subagent::normalize_replaced_tools(&def.replaces),
        tools: def_tool_names(def),
    })
}

/// One resolved def as the roster entry attaching it produces.
///
/// `replaces` and `tools` are copied in here and never re-read: editing the YAML def or the
/// registry assistant afterwards would otherwise silently change what a running session's main
/// agent is allowed to call (PRD § An entry). Detaching and re-attaching is the explicit way to
/// pick an edit up.
fn roster_record(
    def: &tddy_discovery::agent_def::SpecializedAgentDef,
    daemon_instance_id: &str,
) -> Result<tddy_core::SessionAgentRecord, tddy_core::AgentIdError> {
    Ok(tddy_core::SessionAgentRecord {
        agent_id: qualified_agent_id(&def.name, daemon_instance_id)?,
        name: def.name.clone(),
        daemon_instance_id: daemon_instance_id.to_string(),
        label: def.label.clone().filter(|s| !s.trim().is_empty()),
        model: def.model.clone(),
        replaces: tddy_discovery::subagent::normalize_replaced_tools(&def.replaces),
        tools: def_tool_names(def),
        // A local agent works the facilitating daemon's real worktree, so there is no clone to name.
        codebase_session_id: None,
    })
}

/// The qualified id a def resolved on `daemon_instance_id` is addressed by.
///
/// Refused at the point the id is minted when the def's own name contains `@`: such an id parses
/// back as a different pair, so letting it through would put an entry in the roster that routes
/// somewhere the operator never picked.
fn qualified_agent_id(
    name: &str,
    daemon_instance_id: &str,
) -> Result<String, tddy_core::AgentIdError> {
    tddy_core::AgentId {
        name: name.to_string(),
        daemon_instance_id: daemon_instance_id.to_string(),
    }
    .try_qualified()
}

/// The agent a `StartSessionRequest.specialized_agents` entry names.
///
/// The field keeps its wire shape (`repeated string`) and now carries either form: a qualified
/// `name@daemon_instance_id`, or a bare name. A bare name resolves against *this* daemon, and only
/// here — it is the one place where that reading is not a guess, because a start request has never
/// been able to name any other daemon. Attach takes no such reading (PRD § Identity is qualified,
/// always).
fn started_agent_id(
    reference: &str,
    local_instance_id: &str,
) -> Result<tddy_core::AgentId, Status> {
    match tddy_core::AgentId::parse(reference) {
        Ok(id) => Ok(id),
        Err(tddy_core::AgentIdError::Unqualified(_)) => Ok(tddy_core::AgentId {
            name: reference.to_string(),
            daemon_instance_id: local_instance_id.to_string(),
        }),
        Err(e) => Err(Status::invalid_argument(format!("specialized_agents: {e}"))),
    }
}

/// The `.session.yaml` roster a session start records, from the defs its request resolved to.
///
/// Order is the request's, which is the order the roster keeps and the main agent sees.
fn started_roster(
    defs: &[tddy_discovery::agent_def::SpecializedAgentDef],
    local_instance_id: &str,
) -> Result<Vec<tddy_core::SessionAgentRecord>, Status> {
    defs.iter()
        .map(|def| {
            roster_record(def, local_instance_id)
                .map_err(|e| Status::invalid_argument(format!("specialized_agents: {e}")))
        })
        .collect()
}

/// The revision a freshly started session's roster is at: 1 when it was seeded with agents, 0
/// when it was started with none (PRD § Revision, not diff).
pub(crate) fn started_roster_rev(agents: &[tddy_core::SessionAgentRecord]) -> u64 {
    u64::from(!agents.is_empty())
}

/// The refusal an agent owned by another daemon gets in a session's **start** request.
///
/// Attaching one to a live session is supported (see [`ConnectionServiceImpl::roster_record_for`]);
/// seeding one at start is not, and the difference is not arbitrary. A remote agent needs the
/// session's room to exist so its owning daemon can be admitted to it, and the room is opened by the
/// spawn this request has not reached yet — so a seed resolved here would name a clone that could
/// not be provisioned until after the agent process it was meant to configure had started.
///
/// TODO(session-agent-roster): seed a remote agent by attaching it immediately after the spawn has
/// opened the room, so `specialized_agents` and `AttachSessionAgent` accept the same ids
/// (docs/ft/daemon/session-agent-roster.md § Remote agents). Refused, never read as local: a local
/// def of the same name is a *different* agent, and running it would answer from the wrong host
/// without saying so — which is the exact failure qualified ids exist to prevent.
fn remote_agent_at_start_unsupported(agent_id: &str, owning_daemon: &str) -> Status {
    Status::unimplemented(format!(
        "agent '{agent_id}' is owned by daemon '{owning_daemon}'; a session cannot be *started* \
         with an agent from a peer, because that peer is admitted to the session's room and given \
         its clone only once the room exists. Start the session and attach the agent to it."
    ))
}

/// The exec tools a roster agent's own loop serves from the checkout it reads.
///
/// Everything else — `Write`, `StrReplace`, `Delete`, `Shell`, `Await` — is proxied to the
/// facilitating daemon, because there is exactly one worktree that counts and it is that daemon's. A
/// mutation applied to a clone would be overwritten by the next sync tick and would never reach the
/// session's branch (docs/ft/daemon/session-agent-roster.md § Reads are local; writes proxy).
///
/// A name outside the catalog is **not** read-only. The split has to fail closed: a tool this list
/// has never heard of is one nobody has decided about, and running it against a mirror is the
/// outcome that loses work silently.
fn agent_tool_reads_the_clone(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "Read" | "Glob" | "Grep" | "SemanticSearch" | "ReadLints"
    )
}

/// One exec-tool result as an agent's managed-dispatch layer reads it.
///
/// A failure is rendered as the `{is_error, error}` envelope
/// [`tddy_discovery::subagent::CodebaseAccess`] surfaces as `Err`, rather than as a result string —
/// returning the error envelope as if it were a successful result is how a model is told a file
/// contains the words "file not found".
fn dispatch_envelope(response: ExecuteToolResponse) -> String {
    match response.is_error {
        true => {
            serde_json::json!({ "is_error": true, "error": response.error_message }).to_string()
        }
        false => response.result_json,
    }
}

/// The wire spelling of a turn's stop reason.
///
/// ACP's spelling, matched character for character, because that is what the main agent's
/// `subagent_prompt` hands back and a consumer comparing against `"EndTurn"` has no way to learn
/// this daemon chose another.
fn agent_stop_reason(reason: tddy_discovery::subagent::StopReason) -> &'static str {
    match reason {
        tddy_discovery::subagent::StopReason::EndTurn => "EndTurn",
        tddy_discovery::subagent::StopReason::MaxTurnRequests => "MaxTurnRequests",
        tddy_discovery::subagent::StopReason::Cancelled => "Cancelled",
    }
}

/// Refuse an attach whose withdrawal the session could not enforce.
///
/// In a managed-codebase session the main agent's file tools **are** `mcp__tddy-tools__*` — the
/// jail is what puts them there — so a withdrawn tool is refused on the path the call already
/// takes. The main agent of a session that runs no jail holds native tools that never reach
/// `tddy-tools`, so accepting the attach would advertise an enforcement that does not exist, and
/// the operator would believe the main agent had been forced through the agent when it had not
/// (PRD § Enforced at two layers, AC24).
///
/// An agent that replaces nothing has nothing to enforce and attaches to either kind of session.
fn refuse_unenforceable_withdrawal(
    session_id: &str,
    session_dir: &Path,
    record: &tddy_core::SessionAgentRecord,
) -> Result<(), Status> {
    if record.replaces.is_empty() {
        return Ok(());
    }
    let meta = tddy_core::read_session_metadata(session_dir).map_err(|e| {
        Status::not_found(format!(
            "session '{session_id}' has no readable metadata at {}: {e}",
            session_dir.display()
        ))
    })?;
    if meta.sandbox == Some(true) {
        return Ok(());
    }
    Err(Status::failed_precondition(format!(
        "agent '{}' replaces {} on session '{session_id}', which does not run a managed codebase: \
         its main agent calls those tools natively, never through tddy-tools, so the withdrawal \
         could not be enforced. Attach it to a managed-codebase session, or attach an agent that \
         replaces nothing.",
        record.agent_id,
        record.replaces.join(", ")
    )))
}

/// The qualified ids a persisted roster holds, for the resume paths that re-resolve each agent
/// before relaunching the jail.
///
/// Qualified rather than bare: a resume that resolved `explorer` locally would run *this* daemon's
/// `explorer` for an entry the operator attached from another host, and report it under the id they
/// picked. An id naming a peer is refused by resolution instead.
fn roster_agent_ids(agents: &[tddy_core::SessionAgentRecord]) -> Vec<String> {
    agents.iter().map(|a| a.agent_id.clone()).collect()
}

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
    stack_parent: Option<&str>,
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
    room_host: &dyn crate::session_room::SessionRoomHost,
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
        orchestrator_session_id: stack_parent.map(str::to_string),
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

    let chain_base_ref = ConnectionServiceImpl::resolve_chain_base_ref_status(
        &sessions_base,
        stack_parent,
        &repo_root,
        new_branch_name,
    )?;
    let worktree_base_ref =
        tddy_core::select_worktree_base_ref(selected_integration_base_ref, chain_base_ref);

    // Create the real git worktree (blocking: involves git fetch + git worktree add).
    let repo_root_clone = repo_root.clone();
    let session_dir_clone = session_dir.clone();
    let timeout = config.spawn_worker_request_timeout();
    let worktree_path = spawn_blocking_with_timeout(
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

    push_new_branch_to_origin_if_requested(
        create_remote_branch,
        intent,
        &session_dir,
        &worktree_path,
        timeout,
    )
    .await?;

    // The child's branch now exists (and, when requested, is on origin), so a pr-stack
    // orchestrator's planned node can record it — which is what lets this node's descendants be
    // spawned at all, since they base onto `<remote>/<branch>`. Keyed on the effective branch, so a
    // session resuming the branch a node already owns re-links to that node.
    let remote =
        project_storage::effective_remote_name_for_project(&projects_dir, project_id, &repo_root)
            .map_err(|e| Status::internal(e.to_string()))?;
    ConnectionServiceImpl::link_stack_node_to_spawned_branch(
        &sessions_base,
        stack_parent,
        effective_spawn_branch(
            branch_worktree_intent,
            new_branch_name,
            selected_branch_to_work_on,
            &remote,
        ),
        session_id,
    )?;

    let tddy_tools_path = crate::sandbox_session::resolve_tddy_tools_path(
        config
            .claude_cli
            .as_ref()
            .and_then(|c| c.tddy_tools_path.as_deref()),
    );

    let daemon_url = claude_hook_daemon_url(config);

    // Generate a per-session hook token and write .claude/settings.local.json into the
    // worktree. Claude Code reads this file on startup and wires the six lifecycle hooks.
    // Write failure is warn-and-continue so it never blocks the session from starting.
    let hook_token = Uuid::new_v4().to_string();
    write_claude_hooks_settings(
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
    let binary_owned = resolve_start_session_claude_binary(config);
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
        crate::semantic_index::run_semantic_index_blocking(
            &worktree_path,
            &session_dir,
            embedder,
            task_registry,
            session_id,
        )
        .await
        .map_err(|e| Status::internal(format!("semantic index failed: {e}")))?;
        let (key, value) = crate::semantic_index::semantic_index_env(&session_dir);
        env_extra.push((key, value));
    }

    open_session_room_before_spawning_agent(
        room_host,
        "claude-cli",
        session_id,
        &worktree_path,
        &session_dir,
    )
    .await?;

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
    };
    tddy_core::write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {}", e)))?;

    // Optionally expose the PTY via a per-session LiveKit participant so that LiveKit
    // clients (web UI, pty-relay --livekit-url) can use the same bidi-stream path as
    // tool sessions. Falls back gracefully: if LiveKit is not configured the session is
    // still usable via the gRPC connectrpc endpoints.
    let (lk_room, lk_url, lk_server_identity) = if let Some(lk) =
        spawner::livekit_creds_from_config(config)
    {
        let room_name = spawner::resolve_livekit_room_name(lk.common_room.as_deref(), session_id);
        let server_identity = spawner::livekit_server_identity_for_session(
            lk.daemon_instance_id.as_deref(),
            session_id,
        );
        match crate::cli_session_manager::spawn_livekit_bridge(
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
        branch_conflict: None,
    }))
}

impl ConnectionServiceImpl {
    /// Authenticates the caller, resolves the project's main repo on this host, and confirms
    /// `worktree_path` appears in that repo's `git worktree list`, returning the validated worktree
    /// root. Mirrors the `remove_worktree` preamble: authenticate first (invalid session →
    /// `Unauthenticated`), resolve the project (unknown → `NotFound`), then gate on git's worktree
    /// membership so filesystem access never escapes a real worktree.
    fn resolve_listed_worktree(
        &self,
        session_token: &str,
        project_id: &str,
        worktree_path: &str,
    ) -> Result<PathBuf, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }
        let worktree_path_raw = worktree_path.trim();
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
        if !worktrees::worktree_path_is_listed(&main_repo, &worktree_path) {
            log::warn!(
                "resolve_listed_worktree: worktree_path not in git worktree list: {:?}",
                worktree_path
            );
            return Err(Status::failed_precondition(
                "worktree_path is not a worktree of this project",
            ));
        }
        Ok(worktree_path)
    }

    /// Ensure the project's working copy exists on this (local) host before a session starts,
    /// auto-cloning it when missing: from the local registry's `git_url` if the project is already
    /// registered here, otherwise from a peer daemon that hosts it (reusing the logical
    /// `project_id`). The blocking clone runs off the async runtime; a project unknown locally and
    /// on every peer surfaces as `NotFound`.
    ///
    /// When `agent_clone` is set, this daemon is the **owning** side of an agent clone and the
    /// project is provisioned from the facilitating daemon's `remote_git.RemoteGitService` rather
    /// than from a peer-discovered forge URL — `git clone {facilitating_instance_id}:{project_id}`
    /// with `GIT_SSH_COMMAND=tddy-remote-git-repo --daemon-url {facilitating_daemon_url}
    /// --session-token {session_token}`. That closes the two cases the peer fan-out fails for a
    /// remote owning daemon: a peer with no common room of its own, and a project whose `git_url`
    /// names a forge the peer cannot reach (PRD AC37).
    async fn ensure_project_available_for_start(
        &self,
        os_user: &str,
        projects_dir: &Path,
        project_id: &str,
        session_token: &str,
        agent_clone: Option<&tddy_service::proto::connection::AgentClonePlacement>,
    ) -> Result<project_storage::ProjectData, Status> {
        // Resolved lazily: only a peer-provisioned clone needs a base path. A locally-registered
        // project (the common case) never consults it, so a `None` here must not fail the start —
        // it only surfaces as an error if `ensure_project_available_locally` actually needs it.
        let repos_base_dir = repos_base_for_user(os_user, self.config.repos_base_path_or_default());
        let spawn_client = self.spawn_client.clone();
        let os_user_owned = os_user.to_string();
        let projects_dir_owned = projects_dir.to_path_buf();
        let project_id_owned = project_id.to_string();
        let timeout = self.config.spawn_worker_request_timeout();

        // An agent clone provisions from its facilitating daemon directly, so it never fans out
        // `ListProjects` across the common room — the fan-out is the path that fails for a peer
        // with no room of its own, and it is the one AC37 replaces.
        let facilitating = match agent_clone {
            Some(placement) => {
                let facilitating_instance_id = placement.facilitating_daemon_instance_id.trim();
                let facilitating_daemon_url = placement.facilitating_daemon_url.trim();
                if facilitating_instance_id.is_empty() || facilitating_daemon_url.is_empty() {
                    return Err(Status::invalid_argument(format!(
                        "agent_clone placement is incomplete: facilitating_daemon_instance_id and \
                         facilitating_daemon_url are both required to provision a clone on a daemon \
                         that has never seen the project (got instance_id={facilitating_instance_id:?}, \
                         url={facilitating_daemon_url:?})"
                    )));
                }
                Some((
                    facilitating_instance_id.to_string(),
                    facilitating_daemon_url.to_string(),
                ))
            }
            None => None,
        };

        // Peer discovery is an async RPC fan-out; resolve it here rather than inside the blocking
        // clone task. It is only needed when the project is not registered on this host — a
        // locally-registered project clones from its stored `git_url` with no peer lookup — so the
        // fan-out is skipped entirely in that case. An agent clone skips the fan-out unconditionally
        // (it provisions from its facilitator, above).
        let registered_locally = project_storage::find_project(projects_dir, project_id)
            .map_err(|e| Status::internal(format!("read project registry: {e}")))?
            .is_some();
        let peer_entries = if registered_locally || facilitating.is_some() {
            Vec::new()
        } else {
            self.eligible_daemon_source
                .peer_project_entries(session_token)
                .await
        };

        let spawn_backend = crate::supervisor_client::spawn_backend_choice(&self.config);
        // `ensure_project_available_locally` clones synchronously while the supervisor's client is
        // async. Handing the closure a runtime handle keeps that seam here: the closure already runs
        // on a blocking thread, which is precisely where awaiting a future by blocking belongs.
        let runtime = tokio::runtime::Handle::current();

        // The transport-shim env var a facilitator-provisioned clone sets on the `git clone` child.
        // `tddy-remote-git-repo` takes its daemon URL and session token as `--long` flags on the
        // `GIT_SSH_COMMAND` string, so a single env var carries both — the supervisor's
        // `allowed_env_keys` needs only `GIT_SSH_COMMAND` to admit it.
        let ssh_command = facilitating.as_ref().map(|(_, daemon_url)| {
            format!(
                "{} --daemon-url {daemon_url} --session-token {session_token}",
                crate::project_provision::resolve_remote_git_repo_path()
            )
        });
        let facilitating_remote_url = facilitating
            .as_ref()
            .map(|(instance_id, _)| format!("{instance_id}:{project_id_owned}"));

        let handle = tokio::task::spawn_blocking(move || {
            let cloner = |git_url: &str, dest: &Path| -> Result<(), String> {
                match &spawn_backend {
                    crate::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                        let mut env = std::collections::BTreeMap::new();
                        if let Some(ref ssh) = ssh_command {
                            env.insert("GIT_SSH_COMMAND".to_string(), ssh.clone());
                        }
                        runtime
                            .block_on(crate::supervisor_spawn::clone_repo_via_supervisor_with_env(
                                socket_path,
                                &os_user_owned,
                                git_url,
                                dest,
                                env,
                            ))
                            .map_err(|e| format!("{e:#}"))
                    }
                    crate::supervisor_client::SpawnBackendChoice::ForkedWorker => {
                        if let Some(ref client) = spawn_client {
                            if ssh_command.is_none() {
                                // No transport env var to carry: the forked worker's `clone_repo` is
                                // the original path (it has no env-var channel).
                                return client
                                    .clone_repo(spawn_worker::CloneRequest {
                                        os_user: os_user_owned.clone(),
                                        git_url: git_url.to_string(),
                                        destination: dest.display().to_string(),
                                    })
                                    .map_err(|e| e.to_string());
                            }
                        }
                        // Facilitator clone (carries `GIT_SSH_COMMAND`) or no forked worker at all:
                        // the in-process `clone_as_user_with_env` carries the transport env var directly.
                        let extra: Vec<(&str, &str)> = ssh_command
                            .as_ref()
                            .map(|ssh| vec![("GIT_SSH_COMMAND", ssh.as_str())])
                            .unwrap_or_default();
                        spawner::clone_as_user_with_env(&os_user_owned, git_url, dest, &extra)
                            .map_err(|e| e.to_string())
                    }
                }
            };
            if let Some(remote_url) = facilitating_remote_url {
                crate::project_provision::ensure_project_available_from_facilitator(
                    &projects_dir_owned,
                    &project_id_owned,
                    repos_base_dir.as_deref(),
                    &remote_url,
                    cloner,
                )
            } else {
                let peer_lookup = |id: &str| {
                    peer_entries
                        .iter()
                        .find(|p| p.project_id == id)
                        .map(|p| (p.name.clone(), p.git_url.clone()))
                };
                crate::project_provision::ensure_project_available_locally(
                    &projects_dir_owned,
                    &project_id_owned,
                    repos_base_dir.as_deref(),
                    cloner,
                    peer_lookup,
                )
            }
        });

        match tokio::time::timeout(timeout, handle).await {
            Ok(Ok(res)) => res,
            Ok(Err(join_err)) => Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => Err(Status::deadline_exceeded(
                "ensure_project_available_for_start: clone timed out",
            )),
        }
    }

    /// Report — once per `(agents dir, name)` per process — that a registry assistant is shadowing
    /// a `<tddyhome>/agents` def of the same name.
    ///
    /// `create_assistant` refuses a name a def already answers to, so this can only happen the
    /// other way round: the def was written *after* the assistant existed. Resolution deliberately
    /// does **not** refuse in that case. `resolvable_agent_defs` answers `ListAgents`,
    /// `ListSubagents`, `StartSession` and roster attach, so making a name tie fatal here would let
    /// one stray YAML file break every agent listing and every session start on the daemon —
    /// the operator's typo would cost them the daemon. Flipping the winner instead would silently
    /// change which agent an existing session runs, which is the thing the create-time guard exists
    /// to prevent, in the other direction.
    ///
    /// So the ordering stands and the silence is what gets fixed. Deduplicated because this runs on
    /// every `ListAgents`; an undeduplicated line here would flood the log rather than inform it.
    fn report_shadowed_agent_def(agents_dir: &std::path::Path, name: &str) {
        static REPORTED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
            std::sync::OnceLock::new();
        let key = format!("{}\u{0}{name}", agents_dir.display());
        let mut reported = REPORTED
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !reported.insert(key) {
            return;
        }
        log::error!(
            "agent '{name}' is defined both as a registry assistant and by a def in {} — the \
             assistant wins and the def will not resolve. Rename one of them; \
             `--agent {name}` currently runs the assistant.",
            agents_dir.display()
        );
    }

    /// Every agent def a name can resolve against on this daemon: the YAML defs
    /// under `<tddyhome>/agents`, and this daemon's registry assistants — `{builtin, yaml,
    /// sqlite}`. A registry assistant of the same name as a YAML def wins, on the same
    /// "the more specific source is the one the operator just edited" rule that already makes a
    /// YAML def beat a builtin.
    pub async fn resolvable_agent_defs(
        &self,
    ) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        let agents_dir = self.tddy_data_dir.join("agents");
        let mut defs = tddy_discovery::agent_def::resolve_agent_defs(&agents_dir);
        for def in self.registry_agent_defs().await? {
            match defs.iter_mut().find(|d| d.name == def.name) {
                Some(existing) => {
                    Self::report_shadowed_agent_def(&agents_dir, &def.name);
                    *existing = def;
                }
                None => defs.push(def),
            }
        }
        Ok(defs)
    }

    /// The def a session started as `agent` must actually be built from, for `caller`.
    ///
    /// Differs from [`Self::resolvable_agent_defs`] in one way that matters: a registry assistant's
    /// def comes back carrying its provider's credential. The listing path deliberately does not —
    /// `ListAgents` is answered for every operator, and a key has no business in it — but a session
    /// started without one comes up "successfully" and 401s on every model call.
    pub async fn agent_def_for_spawn(
        &self,
        agent: &str,
        caller: &str,
    ) -> Result<Option<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        if let Some(registry) = &self.model_registry {
            // The registry wins over a YAML def of the same name, the same way it does in
            // `resolvable_agent_defs`.
            if let Some(def) =
                crate::model_registry::registry_agent_def_with_credential(registry, agent, caller)
                    .await
                    .map_err(Status::from)?
            {
                return Ok(Some(def));
            }
        }
        let agents_dir = self.tddy_data_dir.join("agents");
        Ok(tddy_discovery::agent_def::resolve_agent_defs(&agents_dir)
            .into_iter()
            .find(|d| d.name == agent))
    }

    /// This daemon's registry assistants as agent defs. Empty when no registry is wired (a test
    /// fixture); a registry that is wired but unreadable is an error, never "no assistants" — a
    /// session started against a name that silently stopped resolving runs as something else.
    async fn registry_agent_defs(
        &self,
    ) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        match &self.model_registry {
            Some(registry) => crate::model_registry::registry_agent_defs(registry)
                .await
                .map_err(Status::from),
            None => Ok(Vec::new()),
        }
    }

    /// Resolve `specialized_agents` references against [`Self::resolvable_agent_defs`] into their
    /// full defs (see docs/ft/coder/specialized-subagents.md). Each entry is either a qualified
    /// `name@daemon_instance_id` or a bare name read as this daemon's (see [`started_agent_id`]).
    /// An unresolvable reference is a request error — the session is never started with a
    /// silently-dropped subagent. An empty input resolves to an empty output, not an error.
    async fn resolve_specialized_agent_defs(
        &self,
        specialized_agents: &[String],
    ) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        if specialized_agents.is_empty() {
            return Ok(Vec::new());
        }
        let local_instance_id = local_instance_id_for_config(&self.config);
        let resolved = self.resolvable_agent_defs().await?;
        let mut selected = Vec::with_capacity(specialized_agents.len());
        for reference in specialized_agents {
            let id = started_agent_id(reference, &local_instance_id)?;
            if id.daemon_instance_id != local_instance_id {
                return Err(remote_agent_at_start_unsupported(
                    reference,
                    &id.daemon_instance_id,
                ));
            }
            let def = resolved.iter().find(|d| d.name == id.name).ok_or_else(|| {
                Status::invalid_argument(format!(
                    "specialized_agents: unknown subagent '{reference}' (not found under \
                     <tddyhome>/agents, and not an assistant in this daemon's registry)"
                ))
            })?;
            selected.push(def.clone());
        }
        Ok(selected)
    }

    /// The session directory a roster call addresses, resolved **only after** its caller has been.
    ///
    /// Auth first is load-bearing rather than tidy: attaching an agent owned by another daemon
    /// contacts that peer and provisions a checkout on it, so a check that ran afterwards would let
    /// an unauthenticated caller build a clone on another host (PRD AC12).
    fn roster_session_dir(&self, session_token: &str, session_id: &str) -> Result<PathBuf, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        // Resolved for the authorization decision alone: a caller who maps to no OS user may not
        // reach a session, but the path itself is this daemon's, not that user's — config is the
        // single source of the sessions base (`sessions_base_for_user`).
        self.config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        self.session_dir_for(session_id)
    }

    /// Where a session this daemon serves keeps its `.session.yaml`.
    ///
    /// The id is validated as a single path segment before it is joined, because every roster call
    /// takes it from the caller and the directory it names is read-modify-written: an id carrying
    /// `../` would have an attach rewrite another user's `.session.yaml` outside this daemon's
    /// sessions base entirely.
    fn session_dir_for(&self, session_id: &str) -> Result<PathBuf, Status> {
        validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        Ok(self.tddy_data_dir.join(SESSIONS_SUBDIR).join(session_id))
    }

    // ── Remote agents: room admission, clones, tool split ────────────────────────────────────
    //
    // docs/ft/daemon/session-agent-roster.md § Remote agents, § Clones.

    /// Open the session's room, if it is not open already, so an owning daemon has something to be
    /// admitted to.
    ///
    /// A session started through the ordinary spawn path already has its room — it is opened before
    /// the agent process exists. This covers the session that does not: a resumed session whose
    /// daemon restarted, and any session whose first remote agent arrives after the room was closed.
    /// A peer told to join a room nobody opened waits out its deadline against a participant that
    /// never arrives, so this is done *before* the peer is asked for anything.
    async fn ensure_session_room_for_agents(
        &self,
        session_id: &str,
        session_dir: &Path,
    ) -> Result<(), Status> {
        if self.session_rooms.hosts(session_id) {
            return Ok(());
        }
        let meta = read_session_metadata(session_dir).map_err(|e| {
            Status::not_found(format!(
                "session '{session_id}' has no readable metadata at {}: {e}",
                session_dir.display()
            ))
        })?;
        let worktree_root = meta.repo_path.as_ref().map(PathBuf::from).ok_or_else(|| {
            Status::failed_precondition(format!(
                "session '{session_id}' has no checkout on this daemon, so its room cannot be \
                 opened here; a remote agent reads a mirror of that checkout and there is nothing \
                 to mirror"
            ))
        })?;
        let local_instance_id = local_instance_id_for_config(&self.config);
        let hosting = crate::session_room::DaemonRoomHosting {
            config: &self.config,
            instance_id: &local_instance_id,
            rooms: &self.session_rooms,
        }
        .for_worktree(session_id, &worktree_root, session_dir);
        match self
            .session_rooms
            .open(
                &hosting,
                tddy_service::ConnectionServiceServer::new(self.clone()),
            )
            .await?
        {
            Some(room) => {
                log::info!(
                    "AttachSessionAgent: opened {} as {} so an owning daemon can be admitted to it",
                    room.room,
                    room.server_identity
                );
                Ok(())
            }
            // A daemon with no LiveKit credentials hosts no rooms, which is fine for a local agent
            // and impossible for a remote one: there would be no room to sync the clone from and no
            // route to the owning daemon.
            None => Err(Status::failed_precondition(format!(
                "session '{session_id}' cannot take an agent from another daemon: this daemon has \
                 no LiveKit configuration, so it hosts no session room for that daemon to join"
            ))),
        }
    }

    /// Claim the clone serving `daemon_instance_id`'s agents on this session, and start building it
    /// if this is the first agent that daemon owns here.
    ///
    /// Returns the clone's `workspace` session id, which the roster entry records. The id is minted
    /// **before** the peer is contacted and travels in the request as `requested_session_id`, so a
    /// forward that never answers still leaves this daemon able to name — and therefore tear down —
    /// whatever the peer built.
    async fn claim_agent_clone(
        &self,
        session_id: &str,
        session_dir: &Path,
        daemon_instance_id: &str,
        session_token: &str,
    ) -> Result<ClaimedAgentClone, Status> {
        // Before the claim: a room this daemon could not open is a clone that could never sync, and
        // a claim recorded for it would leave the roster naming a checkout nobody will build.
        self.ensure_session_room_for_agents(session_id, session_dir)
            .await?;

        let (codebase_session_id, provision) =
            self.session_agent_clones
                .claim(session_id, daemon_instance_id, || {
                    Uuid::now_v7().to_string()
                });
        if !provision {
            return Ok(ClaimedAgentClone {
                codebase_session_id,
                commissioned: false,
            });
        }

        // Spawned rather than awaited: the peer resolves the project — cloning it if it does not
        // have it — and cuts a worktree, which its own `spawn_worker_request_timeout` bounds at five
        // minutes. Holding the attach open for that would make a 90-second `git clone` look like a
        // hung RPC, so the entry is published PROVISIONING and republished when the peer reports.
        let service = self.clone();
        let session_id = session_id.to_string();
        let daemon_instance_id = daemon_instance_id.to_string();
        let session_dir = session_dir.to_path_buf();
        let clone_id = codebase_session_id.clone();
        let session_token = session_token.to_string();
        tokio::spawn(async move {
            if let Err(status) = service
                .provision_agent_clone(
                    &session_id,
                    &session_dir,
                    &daemon_instance_id,
                    &clone_id,
                    &session_token,
                )
                .await
            {
                log::error!(
                    "AttachSessionAgent: could not build session {session_id}'s clone \
                     {clone_id} on daemon {daemon_instance_id}: {}",
                    status.message()
                );
                service.session_agent_clones.fail(
                    &session_id,
                    &daemon_instance_id,
                    status.message(),
                );
            }
            // The attach that commissioned this clone may have failed while the peer was still
            // cutting the checkout. Its unwind cannot delete a session the peer had not created yet,
            // so the side that knows the checkout now exists finishes the job: no claim under this
            // id means nothing will ever read it.
            let still_claimed = service
                .session_agent_clones
                .get(&session_id, &daemon_instance_id)
                .is_some_and(|clone| clone.codebase_session_id == clone_id);
            if !still_claimed {
                log::info!(
                    "AttachSessionAgent: session {session_id} no longer claims clone {clone_id}; \
                     deleting it on daemon {daemon_instance_id}"
                );
                service
                    .delete_clone_on_peer(&daemon_instance_id, &clone_id, &session_token)
                    .await;
                return;
            }
            // Either way the roster now says something new about the clone, and a subscriber that
            // only heard about `rev` changes would show `provisioning` until an attach that may
            // never come.
            service
                .publish_roster_change(&session_id, &session_dir)
                .await;
        });
        Ok(ClaimedAgentClone {
            codebase_session_id,
            commissioned: true,
        })
    }

    /// Give back a clone this attach commissioned, because the attach did not complete.
    ///
    /// Only the call that *commissioned* the checkout may take it away: a second agent on the same
    /// host shares the first one's clone, and unwinding that one would delete a checkout the roster
    /// still names (PRD § One clone per (session, remote daemon)).
    ///
    /// The claim is dropped before the peer is asked, so a checkout the peer creates after this ran
    /// is deleted by the provisioning task that created it rather than left orphaned.
    async fn unwind_agent_clone_claim(
        &self,
        session_id: &str,
        daemon_instance_id: &str,
        claimed: &ClaimedAgentClone,
        session_token: &str,
    ) {
        if !claimed.commissioned {
            return;
        }
        self.session_agent_clones
            .forget(session_id, daemon_instance_id);
        self.delete_clone_on_peer(
            daemon_instance_id,
            &claimed.codebase_session_id,
            session_token,
        )
        .await;
    }

    /// Ask `daemon_instance_id` for the checkout this session's agents on it will read.
    ///
    /// The same `workspace`-session primitive a split placement uses, and for the same reasons: the
    /// peer knows how to provision a project by clone when it does not have one, how to cut a
    /// worktree for it, and how to report and delete it — and an operator sees the result as an
    /// ordinary session rather than as a directory the daemon made up.
    ///
    /// The placement carries this daemon's Connect-HTTP root (`facilitating_daemon_url`) so a peer
    /// that has never seen the project clones it from this daemon's `remote_git.RemoteGitService`
    /// rather than from its own peer fan-out — `git clone {facilitating_instance_id}:{project_id}`
    /// with `GIT_SSH_COMMAND=tddy-remote-git-repo`, the transport `tddy-session-sync` already uses
    /// (`docs/ft/daemon/remote-git-repo.md`). That closes the two cases the fan-out fails: a peer in
    /// this daemon's room that keeps no room of its own has nobody to ask, and a project whose
    /// `git_url` names a forge the peer cannot reach has nothing to clone (PRD AC37).
    async fn provision_agent_clone(
        &self,
        session_id: &str,
        session_dir: &Path,
        daemon_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        let meta = read_session_metadata(session_dir).map_err(|e| {
            Status::not_found(format!(
                "session '{session_id}' has no readable metadata: {e}"
            ))
        })?;
        let slot = self.common_room_slot("AttachSessionAgent")?.clone();
        // The room-admission handshake (PRD § "What attach does" step 3): the facilitating daemon
        // records the owning daemon in the per-session admission registry and mints the scoped,
        // short-TTL token it forwards along with the StartSession. The owning daemon joins
        // `session-{session_id}` with this token and nothing else, then runs the re-admit loop
        // against `session_admission.SessionAdmissionService/AdmitOwningDaemon` before it expires.
        // `None` (LiveKit not configured) falls back to the owning daemon self-minting — a recorded
        // deviation, never a silent one.
        let first_admission = self.mint_first_admission_token(session_id, daemon_instance_id);
        let (first_admission_token, first_admission_url, first_admission_room, _ttl) =
            match first_admission {
                Some((token, url, room, ttl)) => (token, url, room, ttl),
                None => {
                    log::warn!(
                        "session {session_id}: facilitating daemon could not mint an admission \
                         token for owning daemon {daemon_instance_id}; the owning daemon will fall \
                         back to self-minting a room token (PRD § Deviations — no handshake)"
                    );
                    (String::new(), String::new(), String::new(), 0u64)
                }
            };
        let request = StartSessionRequest {
            session_token: session_token.to_string(),
            session_type: "workspace".to_string(),
            project_id: meta.project_id.clone(),
            // Empty: the peer holds this checkout itself and must not route the request onward.
            daemon_instance_id: String::new(),
            codebase_daemon_instance_id: String::new(),
            requested_session_id: codebase_session_id.to_string(),
            agent_clone: Some(tddy_service::proto::connection::AgentClonePlacement {
                session_id: session_id.to_string(),
                facilitating_daemon_instance_id: local_instance_id_for_config(&self.config),
                facilitating_daemon_url: advertise_daemon_url(&self.config),
                first_admission_token,
                first_admission_url,
                first_admission_room,
            }),
            ..StartSessionRequest::default()
        };
        // The split forward's deadline, for the split forward's reason: giving up after the ordinary
        // 30 s would mean erroring while the peer is still cloning, and a peer that carried on would
        // leave a checkout on a host nobody is watching.
        let answered = crate::livekit_peer_discovery::forward_start_session_via_livekit_within(
            &slot,
            daemon_instance_id,
            &request,
            self.split_forward_deadline(),
        )
        .await?;
        let created = answered.session_id.trim();
        if created != codebase_session_id {
            // A peer that ignored `requested_session_id` cannot give the guarantee above, so what it
            // did create is torn down under the id it reported and the clone fails.
            self.delete_clone_on_peer(daemon_instance_id, created, session_token)
                .await;
            return Err(Status::internal(format!(
                "daemon {daemon_instance_id} created workspace session {created:?} instead of the \
                 requested {codebase_session_id:?}; it does not honour requested_session_id, so \
                 this clone could not be reclaimed after a failed attach"
            )));
        }
        // Nothing is marked READY here. The peer joins the session room and restores the checkout
        // from the session's WIP ref after it has answered, and only it can say when that is done —
        // so it reports (`ReportAgentCloneState`) and this daemon waits to be told. Marking readiness
        // from the side that cannot see the checkout is how a prompt gets served from an empty tree.
        log::info!(
            "AttachSessionAgent: daemon {daemon_instance_id} is building session {session_id}'s \
             clone as workspace session {codebase_session_id}"
        );
        Ok(())
    }

    /// Delete a clone's `workspace` session on the daemon holding it.
    ///
    /// Used only to unwind a failed provisioning, where the caller already has the more useful error
    /// to return — so a teardown failure names the orphan in the log rather than replacing it.
    async fn delete_clone_on_peer(
        &self,
        daemon_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
    ) {
        let Ok(slot) = self.common_room_slot("AttachSessionAgent") else {
            log::error!(
                "could not reach the common room to delete workspace session \
                 {codebase_session_id} on daemon {daemon_instance_id}; its checkout is orphaned there"
            );
            return;
        };
        match crate::livekit_peer_discovery::forward_delete_session_via_livekit(
            slot,
            daemon_instance_id,
            &DeleteSessionRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session_id.to_string(),
            },
        )
        .await
        {
            Ok(_) => log::info!(
                "deleted workspace session {codebase_session_id} on daemon {daemon_instance_id}"
            ),
            Err(e) if peer_has_no_such_session(&e) => log::info!(
                "daemon {daemon_instance_id} no longer has workspace session \
                 {codebase_session_id} ({e}); it was already torn down"
            ),
            Err(e) => log::error!(
                "could not delete workspace session {codebase_session_id} on daemon \
                 {daemon_instance_id} ({e}); its checkout is orphaned there"
            ),
        }
    }

    /// Tear down the clone `daemon_instance_id` holds for this session.
    ///
    /// Follows the discipline `docs/ft/daemon/remote-managed-worktree.md` § Teardown established:
    /// the peer answering **"no such session" is success** — the clone is an ordinary listable
    /// session an operator may have deleted directly, and treating that as an error would make the
    /// agent permanently undetachable, with a message naming a checkout that no longer exists. Only
    /// *unreachable or failed* refuses, naming the checkout left behind.
    ///
    /// The refusals say nothing about what the caller has already changed locally, because the two
    /// callers differ: `DetachSessionAgent` has removed and persisted the entry before it gets here,
    /// while `DeleteSession` has deleted nothing yet and is refused outright. Each adds that half
    /// itself.
    async fn tear_down_agent_clone(
        &self,
        session_id: &str,
        daemon_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        let slot = self.common_room_slot("DetachSessionAgent")?;
        // Being *configured* for a common room is not being in one, and the two failures are
        // indistinguishable from their status codes alone: a forward attempted with no room fails
        // locally with `failed_precondition`, exactly as a peer that does not have the session does.
        // Without this check a momentary disconnect would read as "already torn down".
        if slot.read().await.is_none() {
            return Err(Status::failed_precondition(format!(
                "cannot reach the common room to delete session {session_id}'s clone \
                 {codebase_session_id} on daemon {daemon_instance_id}, so its checkout is still \
                 there; delete workspace session {codebase_session_id} on {daemon_instance_id} once \
                 the daemons can see each other"
            )));
        }
        match crate::livekit_peer_discovery::forward_delete_session_via_livekit(
            slot,
            daemon_instance_id,
            &DeleteSessionRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session_id.to_string(),
            },
        )
        .await
        {
            Ok(_) => log::info!(
                "deleted session {session_id}'s clone {codebase_session_id} on daemon \
                 {daemon_instance_id}"
            ),
            Err(e) if peer_has_no_such_session(&e) => log::info!(
                "daemon {daemon_instance_id} no longer has session {session_id}'s clone \
                 {codebase_session_id} ({e}); it was already torn down, so the detach continues"
            ),
            Err(e) => {
                return Err(Status::internal(format!(
                    "could not delete session {session_id}'s clone {codebase_session_id} on daemon \
                     {daemon_instance_id} ({e}); its checkout is left behind there — delete \
                     workspace session {codebase_session_id} on {daemon_instance_id} directly"
                )))
            }
        }
        self.session_agent_clones
            .forget(session_id, daemon_instance_id);
        // The admission this clone's owning daemon held is gone with it: a later `AdmitOwningDaemon`
        // call from that daemon (its mirror's re-admit loop, or a stale retry) must now refuse, and
        // the only way it can refuse is if the registry no longer lists the daemon as admitted. This
        // is the revocation half of the handshake (PRD § "What attach does" step 3) — the detach
        // path. The session-delete path revokes every admitted daemon at once via
        // `revoke_all_for_session`.
        let revoked = self
            .session_admissions
            .revoke(session_id, daemon_instance_id);
        if revoked {
            log::info!(
                "revoked admission for daemon {daemon_instance_id} to session {session_id} \
                 (last agent detached)"
            );
        }
        Ok(())
    }

    /// Delete every clone a session created, on every host that built one.
    ///
    /// Called by `DeleteSession`. One failure fails the deletion naming the orphan, for the same
    /// reason a split session's paired workspace does: a delete that succeeded locally while a
    /// checkout survived on another host is exactly the silent leak the pairing exists to prevent.
    async fn tear_down_every_agent_clone(
        &self,
        session_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        for (daemon_instance_id, clone) in self.session_agent_clones.for_session(session_id) {
            self.tear_down_agent_clone(
                session_id,
                &daemon_instance_id,
                &clone.codebase_session_id,
                session_token,
            )
            .await?;
        }
        Ok(())
    }

    /// Push the session's current roster to its `StreamSessionAgents` subscribers and to its room.
    ///
    /// Best-effort on the room and loud on the stream: a subscriber that cannot be reached is a
    /// consumer running on a stale roster, which is the failure this whole feature exists to
    /// prevent, while a room that is not open is the ordinary state of a session whose daemon has
    /// no LiveKit configuration.
    async fn publish_roster_change(&self, session_id: &str, session_dir: &Path) {
        if let Err(e) = self
            .session_agent_rosters
            .republish(session_id, session_dir)
        {
            log::warn!(
                "could not republish session {session_id}'s roster to its subscribers: {}",
                e.message()
            );
            return;
        }
        let Ok(roster) = self.session_agent_rosters.snapshot(session_id, session_dir) else {
            return;
        };
        self.broadcast_roster(session_id, &roster).await;
    }

    /// Broadcast one roster snapshot on the session room's `session.agents` topic.
    ///
    /// Published once, to the whole room, with no `destination_identities` — the same broadcast
    /// discipline `worktree.activity` follows. Every frame is a whole snapshot that every
    /// participant of the session is entitled to, and addressing it would mean the publisher
    /// deciding who is interested, which it cannot know: a browser tab or a newly admitted owning
    /// daemon joins at any time.
    async fn broadcast_roster(&self, session_id: &str, roster: &SessionAgentRoster) {
        let Some(publisher) = self.session_rooms.agents_publisher(session_id) else {
            return;
        };
        if let Err(e) = publisher.publish(&roster.encode_to_vec()).await {
            log::warn!(
                "could not broadcast session {session_id}'s roster on \
                 {}: {e}",
                crate::session_room::SESSION_AGENTS_TOPIC
            );
        }
    }

    /// The identities currently joined to a session's room, as the LiveKit server reports them.
    ///
    /// Read from the server API rather than from this daemon's own connection: the question is who
    /// is in the room, and only the server can answer it for participants this daemon did not admit.
    pub async fn session_room_participant_identities(
        &self,
        session_id: &str,
    ) -> Result<Vec<String>, Status> {
        let room_name = crate::session_room::session_room_name(session_id);
        let rooms = self.room_roster.list_rooms().await.map_err(Status::from)?;
        let room = rooms
            .into_iter()
            .find(|room| room.name == room_name)
            .ok_or_else(|| {
                Status::not_found(format!(
                    "the LiveKit server has no room called {room_name}; session {session_id} is \
                     not being facilitated in one"
                ))
            })?;
        let mut identities: Vec<String> = room
            .participants
            .into_iter()
            .map(|participant| participant.identity)
            .collect();
        identities.sort();
        Ok(identities)
    }

    /// Where the checkout serving `agent_id` is, on the daemon that owns it.
    ///
    /// Reported by that daemon rather than derived here: this daemon does not have the filesystem
    /// the path names, and a path it computed would describe a directory nobody created.
    pub async fn agent_clone_worktree_path(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<PathBuf, Status> {
        let clone = self.agent_clone_for(session_id, agent_id)?;
        clone.worktree_path.ok_or_else(|| {
            Status::failed_precondition(format!(
                "the daemon owning '{agent_id}' has not reported where session {session_id}'s \
                 clone landed (its state is {:?})",
                clone.state
            ))
        })
    }

    /// Every reconcile the daemon owning `agent_id` has reported for this session's clone.
    ///
    /// A reconcile is never silent: a mirror that repairs itself without saying so hides a real
    /// fault, which here is a second writer nobody knows about.
    pub async fn agent_clone_divergences(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<Vec<String>, Status> {
        Ok(self.agent_clone_for(session_id, agent_id)?.divergences)
    }

    /// The clone serving `agent_id`, found through the roster entry that names its owning daemon.
    fn agent_clone_for(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<crate::session_agent_clone::AgentClone, Status> {
        let session_dir = self.session_dir_for(session_id)?;
        let record = self
            .session_agent_rosters
            .entry(session_id, &session_dir, agent_id)?
            .ok_or_else(|| {
                Status::not_found(format!(
                    "agent '{agent_id}' is not attached to session '{session_id}'"
                ))
            })?;
        self.session_agent_clones
            .get(session_id, &record.daemon_instance_id)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "agent '{agent_id}' is served locally by daemon \
                     '{}', which works the session's own worktree and has no clone",
                    record.daemon_instance_id
                ))
            })
    }

    /// The clone this daemon hosts for `session_id`, when it holds one.
    ///
    /// What makes an exec tool addressed at this daemon for another daemon's session resolvable at
    /// all: the session lives elsewhere, so the ordinary "resolve the worktree from my own sessions
    /// base" would find nothing.
    fn hosted_clone_for(
        &self,
        session_id: &str,
    ) -> Option<Arc<crate::session_agent_clone::HostedClone>> {
        self.hosted_agent_clones.get(session_id)
    }

    /// Serve one exec tool for a session whose checkout this daemon holds as an agent clone.
    ///
    /// This is where the read/write split actually happens, so the agent's own turn loop and an
    /// exec-tool RPC addressed here take exactly one path. A read is answered from the clone with no
    /// round trip — which is the entire reason for placing an agent on this host — and a mutation is
    /// proxied to the facilitating daemon's authoritative worktree.
    ///
    /// Which tree is worked is settled by the clone link rather than by the caller: a hosted clone is
    /// a checkout this daemon built for exactly one session on exactly one peer, at the request of a
    /// `StartSession` it already authenticated, so the request cannot select any other tree and the
    /// OS user the tools run as was settled then. *Who may drive them* is settled by the caller's
    /// session token, which every path into here — the RPC handlers and this daemon's own agent turn
    /// loop — establishes before this is reached: the mutating half proxies to the facilitating
    /// daemon under the clone's stored credential, so an unauthenticated caller reaching here would
    /// be writing into another host's authoritative worktree under a credential it never held.
    ///
    /// TODO(session-agent-roster): narrow that to a session-scoped tool token — audience = this
    /// clone's session, exec-tool methods only — which is the same credential the split placement's
    /// trust model already wants and `docs/dev/TODO.md` already records.
    async fn run_hosted_clone_tool(
        &self,
        req: &ExecuteToolRequest,
        clone: &crate::session_agent_clone::HostedClone,
    ) -> ExecuteToolResponse {
        if !agent_tool_reads_the_clone(&req.tool_name) {
            return match clone
                .execute_tool_on_facilitator(&req.tool_name, &req.args_json)
                .await
            {
                Ok(result_json) => ExecuteToolResponse {
                    result_json,
                    is_error: false,
                    error_message: String::new(),
                    job_id: String::new(),
                    job_running: false,
                },
                // Carried in the response rather than raised, exactly as a locally-run tool failure
                // is: the agent asked for a tool and the tool did not happen, which is a tool result
                // and not a transport failure.
                Err(status) => ExecuteToolResponse {
                    result_json: serde_json::json!({ "error": status.message() }).to_string(),
                    is_error: true,
                    error_message: status.message().to_string(),
                    job_id: String::new(),
                    job_running: false,
                },
            };
        }
        let outcome = tool_engine::execute_tool(
            &clone.worktree_path,
            &req.tool_name,
            &req.args_json,
            &self.task_registry,
            &req.session_id,
        )
        .await;
        ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        }
    }

    /// Turn a freshly created `workspace` checkout into a live mirror of the facilitating daemon's
    /// session.
    ///
    /// Spawned rather than awaited: the caller is the facilitating daemon's forwarded
    /// `StartSession`, and holding that open while this joins a room and restores a whole worktree
    /// would push it past a deadline that is already generous for a `git clone`. The mirror reports
    /// its own readiness afterwards (`ReportAgentCloneState`), which is the only account of it that
    /// can be trusted — nothing on the facilitating daemon can see this checkout.
    async fn start_hosted_agent_clone(
        &self,
        placement: &tddy_service::proto::connection::AgentClonePlacement,
        sessions_base: &Path,
        codebase_session_id: &str,
        project_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        let session_id = placement.session_id.trim();
        let facilitating = placement.facilitating_daemon_instance_id.trim();
        if session_id.is_empty() || facilitating.is_empty() {
            return Err(Status::invalid_argument(
                "agent_clone must name both the session it mirrors and the daemon facilitating it; \
                 half a placement names a room to join with nobody in it to address",
            ));
        }
        let (_common_room, url, api_key, api_secret) =
            crate::livekit_peer_discovery::livekit_common_room_connect_strings(&self.config)
                .map_err(|e| {
                    Status::failed_precondition(format!(
                        "this daemon cannot hold an agent clone: {e}"
                    ))
                })?;
        let worktree_path = workspace_session::resolve_worktree_root_for_session(
            sessions_base,
            codebase_session_id,
        )?;
        // The repository the checkout was cut from, which is where its WIP ref is fetched from.
        let projects_dir = projects_path_for_user(
            self.config
                .os_user_for_github(
                    &(self.user_resolver)(session_token)
                        .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?,
                )
                .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| {
                Status::not_found(format!(
                    "project '{project_id}' is not registered here, so an agent clone of it has \
                     nothing to fetch the session's WIP ref from"
                ))
            })?;

        // Only a checkout that was cloned *from the facilitating daemon* fetches its WIP ref over
        // `tddy-remote-git-repo` — its `origin` is the facilitator's `{instance_id}:{project_id}` URL,
        // the only place that ref lives. A checkout the owning daemon already had on the shared
        // filesystem fetches the ref from that local repo directly (the facilitating daemon
        // published it there), so it must NOT carry the transport-shim env var: `origin` there is the
        // forge URL, which `tddy-remote-git-repo` would try to reach and fail. (PRD AC37.)
        let facilitator_origin_prefix = format!("{facilitating}:");
        let is_facilitator_clone = project.git_url.starts_with(&facilitator_origin_prefix);

        let spec = crate::session_agent_clone::CloneMirrorSpec {
            session_id: session_id.to_string(),
            facilitating_daemon_instance_id: facilitating.to_string(),
            owning_daemon_instance_id: local_instance_id_for_config(&self.config),
            codebase_session_id: codebase_session_id.to_string(),
            worktree_path,
            project_repo_path: PathBuf::from(&project.main_repo_path),
            project_id: project_id.to_string(),
            session_token: session_token.to_string(),
            livekit_url: url,
            livekit_api_key: api_key,
            livekit_api_secret: api_secret,
            facilitating_daemon_url: if is_facilitator_clone {
                let u = placement.facilitating_daemon_url.trim();
                if u.is_empty() {
                    None
                } else {
                    Some(u.to_string())
                }
            } else {
                None
            },
            first_admission_token: placement.first_admission_token.clone(),
            first_admission_url: placement.first_admission_url.clone(),
            first_admission_room: placement.first_admission_room.clone(),
            common_room_slot: self.common_room_livekit_room.clone(),
        };
        let hosted = Arc::clone(&self.hosted_agent_clones);
        let clone_id = codebase_session_id.to_string();
        tokio::spawn(async move {
            if let Err(status) = crate::session_agent_clone::run_clone_mirror(spec, hosted).await {
                // Loud and final: the facilitating daemon has already been told the clone failed
                // (the mirror reports before it returns), and there is nothing here that could
                // repair a room this daemon cannot reach.
                log::error!(
                    "agent clone {clone_id} stopped mirroring: {}",
                    status.message()
                );
            }
        });
        Ok(())
    }

    /// Refuse a prompt to an agent whose checkout is not ready to serve reads, naming the state.
    ///
    /// Queuing it would make a 90-second `git clone` look like a hung agent, and serving it would
    /// read an empty checkout and report "not found" for a file that is simply not there yet
    /// (PRD AC33).
    fn refuse_unready_clone(
        &self,
        session_id: &str,
        record: &tddy_core::SessionAgentRecord,
    ) -> Result<(), Status> {
        use tddy_service::proto::connection::AgentCloneState;
        let clone = self
            .session_agent_clones
            .get(session_id, &record.daemon_instance_id);
        let (state, error) = match clone {
            Some(clone) => (clone.state, clone.error),
            None => (AgentCloneState::Unspecified, String::new()),
        };
        match state {
            AgentCloneState::Ready | AgentCloneState::Local => Ok(()),
            AgentCloneState::Provisioning => Err(Status::failed_precondition(format!(
                "agent '{}' cannot be prompted yet: its clone on daemon '{}' is still \
                 provisioning",
                record.agent_id, record.daemon_instance_id
            ))),
            AgentCloneState::Error => Err(Status::failed_precondition(format!(
                "agent '{}' cannot be prompted: its clone on daemon '{}' is in the error state \
                 ({error})",
                record.agent_id, record.daemon_instance_id
            ))),
            AgentCloneState::Unspecified => Err(Status::failed_precondition(format!(
                "agent '{}' cannot be prompted: this daemon has no clone on daemon '{}' for \
                 session '{session_id}' — the state is unknown, which is not the same as ready",
                record.agent_id, record.daemon_instance_id
            ))),
        }
    }

    /// Refuse to address an owning daemon that is no longer in the common room.
    ///
    /// Named rather than left to a forward deadline: an agent on a departed daemon must fail *its
    /// own* prompts with an error naming that daemon, while the rest of the roster keeps working
    /// (PRD AC35). Waiting out `PEER_FORWARD_TIMEOUT` reaches the same answer thirty seconds later
    /// and tells the operator only that something timed out.
    async fn refuse_departed_daemon(&self, daemon_instance_id: &str) -> Result<(), Status> {
        if self
            .eligible_instance_ids()
            .iter()
            .any(|candidate| candidate == daemon_instance_id)
        {
            return Ok(());
        }
        Err(Status::unavailable(format!(
            "daemon '{daemon_instance_id}' has left the common room, so the agents it owns on this \
             session cannot be reached; the rest of the roster is unaffected"
        )))
    }

    /// Ask the owning daemon to open the conversation on its side, under the id this daemon minted.
    ///
    /// The id travels rather than being minted there, so a forward that times out still leaves this
    /// daemon able to name — and therefore cancel — whatever the peer opened.
    async fn forward_open_agent_conversation(
        &self,
        req: &OpenAgentConversationRequest,
        record: &tddy_core::SessionAgentRecord,
        conversation_id: &str,
    ) -> Result<(), Status> {
        let slot = self.common_room_slot("OpenAgentConversation")?;
        let forwarded = OpenAgentConversationRequest {
            conversation_id: conversation_id.to_string(),
            daemon_instance_id: record.daemon_instance_id.clone(),
            ..req.clone()
        };
        let answered = crate::livekit_peer_discovery::forward_to_peer(
            slot,
            &record.daemon_instance_id,
            "connection.ConnectionService",
            "OpenAgentConversation",
            forwarded.encode_to_vec(),
        )
        .await?;
        let opened = OpenAgentConversationResponse::decode(answered.as_slice())
            .map_err(|e| Status::internal(format!("decode OpenAgentConversationResponse: {e}")))?;
        if opened.conversation_id != conversation_id {
            return Err(Status::internal(format!(
                "daemon '{}' opened conversation {:?} instead of the requested {conversation_id:?}, \
                 so a prompt to it could not be routed and a cancel could not name it",
                record.daemon_instance_id, opened.conversation_id
            )));
        }
        Ok(())
    }

    /// A turn loop for an agent this daemon resolves and serves from the session's own worktree.
    async fn open_local_agent_session(
        &self,
        session_id: &str,
        record: &tddy_core::SessionAgentRecord,
        session_token: &str,
    ) -> Result<Box<dyn tddy_discovery::subagent::SubagentSession>, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        // Through the spawn resolver rather than the listing one: a registry assistant's def comes
        // back carrying its provider's credential there, and a session opened without one comes up
        // "successfully" and 401s on every model call.
        let def = self
            .agent_def_for_spawn(&record.name, &github_user)
            .await?
            .ok_or_else(|| {
                Status::invalid_argument(format!(
                    "agent '{}' resolves to no def on this daemon any more",
                    record.agent_id
                ))
            })?;
        Ok(Box::new(
            tddy_discovery::subagent::SpecializedSubagentSession::new(
                def.base_url.clone(),
                def.model.clone(),
                def.api_key.clone(),
                def.max_turns,
                self.local_agent_codebase_access(session_id, session_token),
                def.system_prompt.clone(),
                def.tools.clone(),
            ),
        ))
    }

    /// A turn loop for an agent **this** daemon owns, reading the clone it holds for another
    /// daemon's session.
    async fn open_owned_agent_session(
        &self,
        agent_id: &str,
        clone: &Arc<crate::session_agent_clone::HostedClone>,
    ) -> Result<Box<dyn tddy_discovery::subagent::SubagentSession>, Status> {
        let id = tddy_core::AgentId::parse(agent_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let local_instance_id = local_instance_id_for_config(&self.config);
        if id.daemon_instance_id != local_instance_id {
            return Err(Status::invalid_argument(format!(
                "agent '{agent_id}' is owned by daemon '{}', not by this one ('{local_instance_id}')",
                id.daemon_instance_id
            )));
        }
        let def = self
            .resolvable_agent_defs()
            .await?
            .into_iter()
            .find(|d| d.name == id.name)
            .ok_or_else(|| {
                Status::invalid_argument(format!(
                    "agent '{agent_id}' resolves to no def on daemon '{local_instance_id}'"
                ))
            })?;
        Ok(Box::new(
            tddy_discovery::subagent::SpecializedSubagentSession::new(
                def.base_url.clone(),
                def.model.clone(),
                def.api_key.clone(),
                def.max_turns,
                self.owned_agent_codebase_access(clone),
                def.system_prompt.clone(),
                def.tools.clone(),
            ),
        ))
    }

    /// How an agent this daemon owns reaches files: reads from its clone, mutations proxied.
    ///
    /// Managed rather than [`CodebaseAccess::Local`] even though the checkout is on this host: the
    /// tool engine is what confines a path to the worktree, and a loop given direct filesystem
    /// access would be one YAML field away from writing anywhere this daemon can.
    fn owned_agent_codebase_access(
        &self,
        clone: &Arc<crate::session_agent_clone::HostedClone>,
    ) -> tddy_discovery::subagent::CodebaseAccess {
        let service = self.clone();
        let clone = Arc::clone(clone);
        tddy_discovery::subagent::CodebaseAccess::managed(move |tool_name, args| {
            let service = service.clone();
            let clone = Arc::clone(&clone);
            Box::pin(async move {
                let request = ExecuteToolRequest {
                    session_token: String::new(),
                    session_id: clone.session_id.clone(),
                    daemon_instance_id: String::new(),
                    tool_name,
                    args_json: args.to_string(),
                };
                dispatch_envelope(service.run_hosted_clone_tool(&request, &clone).await)
            })
        })
    }

    /// How an agent this daemon serves locally reaches files: the session's own worktree, through
    /// the same tool engine every other exec-tool caller goes through.
    fn local_agent_codebase_access(
        &self,
        session_id: &str,
        session_token: &str,
    ) -> tddy_discovery::subagent::CodebaseAccess {
        let service = self.clone();
        let session_token = session_token.to_string();
        let session_id = session_id.to_string();
        tddy_discovery::subagent::CodebaseAccess::managed(move |tool_name, args| {
            let service = service.clone();
            let session_token = session_token.clone();
            let session_id = session_id.clone();
            Box::pin(async move {
                let request = ExecuteToolRequest {
                    session_token,
                    session_id,
                    daemon_instance_id: String::new(),
                    tool_name,
                    args_json: args.to_string(),
                };
                match service.resolve_exec_tool_worktree(&request) {
                    Ok((sessions_base, worktree_root)) => dispatch_envelope(
                        service
                            .run_exec_tool_locally(&request, &sessions_base, &worktree_root)
                            .await,
                    ),
                    Err(status) => {
                        serde_json::json!({ "is_error": true, "error": status.message() })
                            .to_string()
                    }
                }
            })
        })
    }

    /// Close every open conversation with `agent_id`.
    ///
    /// An in-flight `prompt` returns an error naming the closure rather than hanging or returning a
    /// partial answer as if complete: the caller is the main agent, and a truncated answer accepted
    /// as whole is the failure that reaches the operator as a wrong review (PRD § What detach does,
    /// step 2).
    ///
    /// A conversation whose loop runs on another daemon is cancelled *there* as well as forgotten
    /// here. Dropping the routing record alone would leave the owning daemon's turn loop running
    /// against a clone this detach is about to delete, with nothing left on this side able to name
    /// it.
    async fn cancel_conversations_with(
        &self,
        session_token: &str,
        session_id: &str,
        agent_id: &str,
    ) {
        let mut cancelled_remotely: Vec<(String, String)> = Vec::new();
        {
            let mut open = self.agent_conversations.lock().await;
            open.retain(|conversation_id, conversation| {
                if !conversation.is_with(session_id, agent_id) {
                    return true;
                }
                match conversation {
                    AgentConversation::Local { closed, .. } => closed.notify_one(),
                    AgentConversation::Remote {
                        daemon_instance_id, ..
                    } => cancelled_remotely
                        .push((daemon_instance_id.clone(), conversation_id.clone())),
                }
                false
            });
        }

        for (daemon_instance_id, conversation_id) in cancelled_remotely {
            if let Err(e) = self
                .forward_cancel_agent_conversation(
                    session_token,
                    session_id,
                    &daemon_instance_id,
                    &conversation_id,
                )
                .await
            {
                // Loud and non-fatal: the entry is already gone here, so failing the detach would
                // leave the roster and this map disagreeing about an agent that is no longer on it.
                log::error!(
                    "could not cancel conversation {conversation_id} with '{agent_id}' on daemon \
                     {daemon_instance_id} ({}); its turn loop may still be running there",
                    e.message()
                );
            }
        }
    }

    /// Ask `daemon_instance_id` to cancel a conversation its own turn loop is running.
    async fn forward_cancel_agent_conversation(
        &self,
        session_token: &str,
        session_id: &str,
        daemon_instance_id: &str,
        conversation_id: &str,
    ) -> Result<(), Status> {
        let slot = self.common_room_slot("CancelAgentConversation")?;
        crate::livekit_peer_discovery::forward_to_peer(
            slot,
            daemon_instance_id,
            "connection.ConnectionService",
            "CancelAgentConversation",
            CancelAgentConversationRequest {
                // The detaching caller's own token: the peer authenticates a cancel exactly as it
                // authenticated the open, and this daemon holds no other credential to present.
                session_token: session_token.to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: daemon_instance_id.to_string(),
                conversation_id: conversation_id.to_string(),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    /// The roster entry a qualified `agent_id` attaches as.
    ///
    /// The id must be qualified: there is deliberately no "assume the local daemon" reading, which
    /// is the reading that silently picks the wrong host the moment two daemons offer a def of the
    /// same name.
    async fn roster_record_for_agent_id(
        &self,
        agent_id: &str,
    ) -> Result<tddy_core::SessionAgentRecord, Status> {
        let id = tddy_core::AgentId::parse(agent_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.roster_record_for(&id, agent_id).await
    }

    /// The roster entry `id` resolves to, reporting any refusal under `named_as` — the string the
    /// caller actually sent, so an operator is never sent looking for an id they never typed.
    async fn roster_record_for(
        &self,
        id: &tddy_core::AgentId,
        named_as: &str,
    ) -> Result<tddy_core::SessionAgentRecord, Status> {
        let local_instance_id = local_instance_id_for_config(&self.config);
        if id.daemon_instance_id != local_instance_id {
            return self.remote_roster_record_for(id, named_as).await;
        }
        let def = self
            .resolvable_agent_defs()
            .await?
            .into_iter()
            .find(|d| d.name == id.name)
            .ok_or_else(|| {
                Status::invalid_argument(format!(
                    "agent '{named_as}' resolves to no def on daemon '{local_instance_id}' (not \
                     found under <tddyhome>/agents, and not an assistant in its registry)"
                ))
            })?;
        roster_record(&def, &local_instance_id).map_err(|e| Status::invalid_argument(e.to_string()))
    }

    /// The roster entry an id naming a **peer** resolves to, taken from that peer's own
    /// `ListSubagents`.
    ///
    /// Resolved there and nowhere else, deliberately: a local def of the same name is a *different*
    /// agent, and answering from it would run the wrong host's agent under the id the operator
    /// picked — the exact failure qualified ids exist to prevent (PRD § What attach does, step 1).
    ///
    /// The peer must be visible in the common room *before* it is asked. A daemon that resolves on
    /// no host is `INVALID_ARGUMENT` naming the id, decided from the eligible-daemon list rather
    /// than by waiting out a forward deadline against a participant that was never there — the
    /// caller mistyped an id, which is a bad request and not a slow one.
    async fn remote_roster_record_for(
        &self,
        id: &tddy_core::AgentId,
        named_as: &str,
    ) -> Result<tddy_core::SessionAgentRecord, Status> {
        let owning_daemon = id.daemon_instance_id.as_str();
        if !self
            .eligible_instance_ids()
            .iter()
            .any(|candidate| candidate == owning_daemon)
        {
            return Err(Status::invalid_argument(format!(
                "agent '{named_as}' is owned by daemon '{owning_daemon}', which is not in this \
                 daemon's common room; nothing was attached and no checkout was created"
            )));
        }

        let slot = self.common_room_slot("AttachSessionAgent")?;
        let answered = crate::livekit_peer_discovery::forward_to_peer(
            slot,
            owning_daemon,
            "connection.ConnectionService",
            "ListSubagents",
            ListSubagentsRequest {}.encode_to_vec(),
        )
        .await
        .map_err(|e| {
            Status::unavailable(format!(
                "daemon '{owning_daemon}' owns agent '{named_as}' but did not answer \
                 ListSubagents ({}); nothing was attached and no checkout was created",
                e.message()
            ))
        })?;
        let listed = ListSubagentsResponse::decode(answered.as_slice())
            .map_err(|e| Status::internal(format!("decode ListSubagentsResponse: {e}")))?;

        let row = listed
            .subagents
            .into_iter()
            .find(|s| s.name == id.name)
            .ok_or_else(|| {
                Status::invalid_argument(format!(
                    "agent '{named_as}' resolves to no def on daemon '{owning_daemon}' (it \
                     answered with no subagent called '{}')",
                    id.name
                ))
            })?;

        // The peer stamps the id it minted, and it is taken verbatim rather than reassembled here:
        // an id the two sides spelled differently routes to a daemon the operator never picked.
        let agent_id = match row.agent_id.trim().is_empty() {
            true => qualified_agent_id(&row.name, owning_daemon)
                .map_err(|e| Status::invalid_argument(e.to_string()))?,
            false => row.agent_id,
        };
        Ok(tddy_core::SessionAgentRecord {
            agent_id,
            name: row.name,
            daemon_instance_id: owning_daemon.to_string(),
            label: Some(row.label).filter(|l| !l.is_empty()),
            model: row.model,
            replaces: row.replaces,
            tools: row.tools,
            // Filled in by the caller once the clone for (session, owning daemon) is claimed: the
            // record is resolved before this daemon knows which session it is being attached to.
            codebase_session_id: None,
        })
    }

    /// Build the `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON` jail env pair for already-resolved
    /// specialized-agent defs (see [`Self::resolve_specialized_agent_defs`]). Empty input produces
    /// no env pairs.
    ///
    /// TODO(session-agent-roster): the in-jail runner derives `--allowedTools` /
    /// `--disallowedTools` from these seeded defs, so a def whose `replaces` was edited between the
    /// attach and the relaunch changes what the relaunched main agent may call — the one thing
    /// snapshotting `replaces` into the roster exists to prevent. Closing it means handing the
    /// runner the roster's replaced set outright instead of letting it re-derive one
    /// (docs/ft/daemon/session-agent-roster.md AC25).
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

    /// Tell the jail which daemon facilitates it.
    ///
    /// Exported unconditionally, and not folded into [`Self::specialized_subagent_env`] which is
    /// skipped for a session that starts with no agents: the roster is mutated while the session
    /// runs, so a jail started empty still needs to be able to qualify what it is later told.
    ///
    /// Without it the in-jail `tddy-tools` cannot qualify its seeded agent ids — a seed resolved on
    /// this daemon is `explorer`, and the id the main agent must type is `explorer@{this daemon}`.
    /// The two other transports carry the id in their own environment
    /// (`TDDY_REMOTE_DAEMON_INSTANCE_ID`, the HTTP daemon's own); a sandbox-IPC jail is told nothing
    /// at all, and a bare id resolves against whichever daemon happens to answer.
    ///
    /// Named after the daemon's own `TDDY_DAEMON_INSTANCE_ID` startup override, so the value and the
    /// variable an operator would set to change it are spelled the same on both sides.
    fn jail_daemon_identity_env(&self) -> Vec<(String, String)> {
        vec![(
            "TDDY_DAEMON_INSTANCE_ID".to_string(),
            local_instance_id_for_config(&self.config),
        )]
    }

    /// The `TDDY_LSP_TOOLS` jail env pair — set when a language server is available for the
    /// session's worktree, so the in-jail `tddy-tools --mcp` exposes the `Lsp*` tools.
    fn lsp_tools_env(&self, worktree_root: &std::path::Path) -> Vec<(String, String)> {
        let available = tddy_core::toolcall::lsp::lsp_executor()
            .map(|ex| ex.is_available(worktree_root))
            .unwrap_or(false);
        if available {
            vec![("TDDY_LSP_TOOLS".to_string(), "rust".to_string())]
        } else {
            Vec::new()
        }
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
        // Client-supplied local checkout to run against directly (StartSessionRequest.repo_path).
        // When non-empty it wins over `project_id`: the session's worktree IS this path (no git
        // worktree is created, no registered project is required, and it is never removed on
        // session end). Empty → resolve the worktree from the registered `project_id` as before.
        repo_path: &str,
        branch_worktree_intent: &str,
        new_branch_name: &str,
        selected_integration_base_ref: &str,
        selected_branch_to_work_on: &str,
        // Passed to `claude` as a trailing positional (first user turn) after any pass-through args.
        initial_prompt: &str,
        // Extra args forwarded verbatim to the in-jail `claude` (StartSessionRequest.claude_args).
        claude_args: &[String],
        permission_mode: &str,
        dangerously_skip_permissions: bool,
        stack_parent: Option<&str>,
        // Specialized subagents (see docs/ft/coder/specialized-subagents.md). This sandboxed path
        // already never mounts the repo (`mounts: vec![]` below, unconditionally) —
        // `managed_codebase` is accepted for request-shape/UI-intent clarity, not to toggle mount
        // behavior. Names resolve against `<tddyhome>/agents` (+ builtins) and are wired into the
        // jail env; all configuration (model, base_url, max_turns, replaces) comes exclusively
        // from the resolved def.
        _managed_codebase: bool,
        specialized_agents: &[String],
        // When `Some`, launch workflow-aware: inject the recipe's orchestration prompt and route the
        // agent's host-side `tddy-tools transition` to a per-session `WorkflowController`.
        managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>>,
        // When true, index the worktree before launch (blocking; aborts on failure) and expose the
        // in-jail `SemanticSearch` tool backed by that per-session index.
        semantic_index: bool,
        // When true (new_branch_from_base + registered project), push the new branch to origin.
        create_remote_branch: bool,
    ) -> Result<Response<StartSessionResponse>, Status> {
        if model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for claude-cli sessions",
            ));
        }
        let project_id = project_id.trim();
        let repo_path = repo_path.trim();
        let specialized_defs = self
            .resolve_specialized_agent_defs(specialized_agents)
            .await?;

        // Readiness gate: wake every specialized agent's endpoint and wait until each answers
        // before spawning the jail, so a cold/unreachable model fails session start here rather
        // than stalling the main agent's first subagent call. No fallback — the jail is never
        // spawned if warm-up fails. Resume gates separately, in `relaunch_sandboxed_runner`.
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;

        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

        // A client-supplied `repo_path` runs against that checkout directly (no registered
        // project), so a stored default branch only applies when resolving from `project_id`.
        let project_default_branch_ref: Option<String> =
            if repo_path.is_empty() && !project_id.is_empty() {
                projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                    .and_then(|dir| {
                        project_storage::find_project(&dir, project_id)
                            .ok()
                            .flatten()
                    })
                    .and_then(|p| p.main_branch_ref)
            } else {
                None
            };

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
            project_default_branch_ref.as_deref(),
        )?;
        let mut cs = Changeset {
            workflow: Some(cs_workflow),
            orchestrator_session_id: stack_parent.map(str::to_string),
            recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
            ..Changeset::default()
        };
        if let Some(recipe) = &managed_recipe {
            tddy_core::changeset::update_state(
                &mut cs,
                tddy_core::workflow::ids::WorkflowState::new(recipe.start_goal().as_str()),
            );
        }
        tddy_core::write_changeset(&session_dir, &cs)
            .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;

        // Resolve the session's worktree. A client-supplied `repo_path` is used directly (arbitrary
        // local checkout, edited via the host-side tool relay as the caller's mapped OS user); it is
        // never wrapped in a daemon-managed git worktree and never removed on session end. Otherwise
        // fall back to the registered project and create a git worktree as before.
        let worktree_path = match session_worktree_source(repo_path, project_id) {
            WorktreeSource::Project(pid) => {
                if pid.is_empty() {
                    return Err(Status::invalid_argument(
                        "project_id is required for claude-cli sessions",
                    ));
                }
                let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                    .ok_or_else(|| Status::internal("could not resolve projects path"))?;
                let project = project_storage::find_project(&projects_dir, &pid)
                    .map_err(|e| Status::internal(e.to_string()))?
                    .ok_or_else(|| Status::not_found("project not found"))?;
                let repo_root = PathBuf::from(&project.main_repo_path);
                if !repo_root.exists() {
                    return Err(Status::invalid_argument(
                        "project main repo path does not exist",
                    ));
                }
                let chain_base_ref = Self::resolve_chain_base_ref_status(
                    &sessions_base,
                    stack_parent,
                    &repo_root,
                    new_branch_name,
                )?;
                let worktree_base_ref = tddy_core::select_worktree_base_ref(
                    selected_integration_base_ref,
                    chain_base_ref,
                );
                let repo_root_clone = repo_root.clone();
                let session_dir_clone = session_dir.clone();
                let timeout = self.config.spawn_worker_request_timeout();
                let wt = spawn_blocking_with_timeout(
                    timeout,
                    "start_sandboxed_claude_cli_session: create worktree",
                    move || {
                        tddy_core::setup_worktree_for_session_with_optional_chain_base(
                            &repo_root_clone,
                            &session_dir_clone,
                            worktree_base_ref.as_deref(),
                        )
                        .map_err(|e| anyhow::anyhow!("worktree setup failed: {e}"))
                    },
                )
                .await?;
                push_new_branch_to_origin_if_requested(
                    create_remote_branch,
                    intent,
                    &session_dir,
                    &wt,
                    timeout,
                )
                .await?;
                // The branch this spawn works on now exists — record it on the orchestrator's planned
                // node (see `link_stack_node_to_spawned_branch`), keyed on the effective branch so a
                // resumed branch re-links its node. Only this arm resolves a project worktree; a
                // client-supplied `repo_path` materializes no planned node.
                let remote = project_storage::effective_remote_name_for_project(
                    &projects_dir,
                    &pid,
                    &repo_root,
                )
                .map_err(|e| Status::internal(e.to_string()))?;
                Self::link_stack_node_to_spawned_branch(
                    &sessions_base,
                    stack_parent,
                    effective_spawn_branch(
                        branch_worktree_intent,
                        new_branch_name,
                        selected_branch_to_work_on,
                        &remote,
                    ),
                    session_id,
                )?;
                wt
            }
            WorktreeSource::RepoPath(path) => {
                let canonical = std::fs::canonicalize(&path).map_err(|e| {
                    Status::invalid_argument(format!(
                        "repo_path {} is not accessible: {e}",
                        path.display()
                    ))
                })?;
                if !canonical.is_dir() {
                    return Err(Status::invalid_argument(format!(
                        "repo_path {} is not a directory",
                        canonical.display()
                    )));
                }
                log::info!(
                    target: "tddy_daemon::connection_service",
                    "start_sandboxed_claude_cli_session {session_id}: using client-supplied repo_path {} directly as worktree (not daemon-managed; not removed on session end)",
                    canonical.display()
                );
                canonical
            }
        };

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
        // scratch_home (jail $HOME) is the persistent daemon-wide claude home, resolved and mounted
        // below — not a per-session dir — so auth/history persist across sessions.
        let scratch_tmp = scratch_dir.join("tmp");
        let context_dir = sandbox_root.join("context");

        // The roster the session starts with, built before the jail rather than at the metadata
        // write below: it is what the withdrawal is computed from, and a request naming a def whose
        // id cannot be minted must fail before a jail exists rather than after.
        let started_agents = started_roster(
            &specialized_defs,
            &local_instance_id_for_config(&self.config),
        )?;
        let replacement_pairs = roster_replacement_pairs(&started_agents);
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

        // Managed workflow: build the per-session controller + toolcall listener, write the recipe's
        // orchestration prompt into the jail-visible context dir, and prepare the per-session env
        // (TDDY_SOCKET + PATH) applied to host-side `tddy-tools transition` (run via the Shell relay).
        let mut managed: Option<crate::session_toolcall::ManagedWorkflow> = None;
        let mut append_system_prompt_file: Option<PathBuf> = None;
        let mut session_env: Vec<(String, String)> = Vec::new();
        if let Some(recipe) = managed_recipe.clone() {
            // A grill-me session gets a conversation-spawn handler bound to its toolcall listener so
            // the agent's `spawn_conversation` relay can start a fresh implementation conversation.
            let conversation_spawn_handler = self.conversation_spawn_handler_for(
                &recipe,
                os_user,
                session_id,
                project_id,
                &sessions_base,
                &session_dir,
            );
            let launch = self.prepare_managed_workflow(
                session_id,
                recipe,
                &session_dir,
                &worktree_path,
                &context_dir,
                &tddy_tools_path,
                None,
                conversation_spawn_handler,
            )?;
            append_system_prompt_file = Some(launch.prompt_file);
            session_env = launch.env;
            managed = Some(launch.workflow);
        }

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
        // Resolve the real `claude` to an absolute path (skipping wrapper shims). Overridable via
        // TDDY_CLAUDE_BINARY or `claude_cli.binary_path`. A bare name would give binary_exec_reads
        // an empty parent → `(subpath "")` → macOS sandbox-exec rejects the profile.
        let claude_binary = crate::config::resolve_claude_binary_path(&self.config);
        let claude_binary = claude_binary.as_str();

        // Persistent daemon-wide jail $HOME: reused across sessions and mounted read-write below, so
        // refreshed OAuth tokens, session history, and settings survive. Seeded non-clobbering.
        let claude_home_dir = crate::config::resolve_claude_home_dir(&self.config);
        let scratch_home =
            crate::sandbox_session::prepare_persistent_claude_home(&claude_home_dir, claude_binary);

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

        let mut runner_argv = vec![
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
        // The runner reconciles this with --permission-mode (they are mutually exclusive; when set,
        // the in-jail claude argv drops --permission-mode). See build_claude_base_argv.
        if dangerously_skip_permissions {
            runner_argv.push("--dangerously-skip-permissions".into());
        }
        if let Some(prompt_path) = &append_system_prompt_file {
            runner_argv.push("--append-system-prompt-file".into());
            runner_argv.push(prompt_path.to_string_lossy().to_string());
        }
        // Forward client-supplied pass-through args (+ a trailing positional prompt) to the in-jail
        // `claude`. Each token becomes a `--claude-arg` occurrence the runner replays verbatim after
        // claude's fixed flags and before the MCP allowlist args.
        for token in sandbox_claude_passthrough_args(claude_args, initial_prompt) {
            runner_argv.push("--claude-arg".into());
            runner_argv.push(token);
        }

        // Semantic index: index the worktree into the session dir before spawning the jail
        // (blocking; a missing embedder or a failed index aborts the start — no unindexed
        // fallback), and inject `TDDY_SEMANTIC_INDEX_DB` into the jail env. Its presence both points
        // the in-jail `SemanticSearch` tool at the per-session index and signals the runner to keep
        // `SemanticSearch` in the tool set (it is otherwise folded into the replaced set).
        let mut semantic_index_env_pair: Option<(String, String)> = None;
        if semantic_index {
            let embedder =
                tddy_semantic_index::production_embedder(&self.tddy_data_dir).map_err(|e| {
                    Status::failed_precondition(format!(
                        "semantic index requested but no embedder is available: {e}"
                    ))
                })?;
            crate::semantic_index::run_semantic_index_blocking(
                &worktree_path,
                &session_dir,
                embedder,
                &self.task_registry,
                session_id,
            )
            .await
            .map_err(|e| Status::internal(format!("semantic index failed: {e}")))?;
            semantic_index_env_pair = Some(crate::semantic_index::semantic_index_env(&session_dir));
        }

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
        env.extend(self.jail_daemon_identity_env());
        env.extend(self.lsp_tools_env(&worktree_path));
        env.extend(semantic_index_env_pair);

        // The jail is this daemon's child and the checkout is this daemon's, so a sandboxed session
        // is facilitated here exactly as an unsandboxed one is — the jail changes what the agent can
        // reach, not who hosts its room.
        open_session_room_before_spawning_agent(
            &self.session_room_host(),
            "claude-cli",
            session_id,
            &worktree_path,
            &session_dir,
        )
        .await?;

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
                // Mount the persistent jail $HOME read-write so it survives the session.
                mounts: vec![tddy_sandbox::MountSpec::read_write(scratch_home.clone())],
                // Persistent home is seeded separately (non-clobbering); disable the recipe's
                // per-session credential copy so it can't overwrite a refreshed jail token.
                host_home: None,
                cgroup: self.config.sandbox_cgroup_config(),
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
        let capture = Arc::new(StdMutex::new(TerminalCapture::new()));
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel();

        crate::sandbox_session::dial_and_bridge(
            session_id,
            worktree_path.clone(),
            &mut handle,
            self.task_registry.clone(),
            stdout_tx.clone(),
            Arc::clone(&capture),
            stdin_rx,
            Arc::new(session_env),
            session_dir.clone(),
            self.agent_activity_hub(),
            Arc::new(DaemonRpcHandler {
                conn: self.self_arc(),
            }),
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
                managed_workflow: managed,
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
            cursor_chat_id: None,
            activity_status: None,
            hook_token: None,
            sandbox: Some(true),
            agent: None,
            recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
            agents_rev: started_roster_rev(&started_agents),
            agents: started_agents,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
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
            branch_conflict: None,
        }))
    }

    /// Handle `StartSession` for sandboxed `cursor-cli` sessions (darwin Seatbelt / Linux cgroups).
    #[allow(clippy::too_many_arguments)]
    async fn start_sandboxed_cursor_cli_session(
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
        stack_parent: Option<&str>,
        initial_prompt: &str,
        _managed_codebase: bool,
        specialized_agents: &[String],
        managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>>,
        // When true, index the worktree before launch (blocking; aborts on failure) and point the
        // in-jail `SemanticSearch` tool at the per-session index.
        semantic_index: bool,
        // When true (new_branch_from_base only), push the new branch to origin at session start.
        create_remote_branch: bool,
    ) -> Result<Response<StartSessionResponse>, Status> {
        if model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for cursor-cli sessions",
            ));
        }
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument(
                "project_id is required for cursor-cli sessions",
            ));
        }
        let specialized_defs = self
            .resolve_specialized_agent_defs(specialized_agents)
            .await?;

        // Readiness gate: wake every specialized agent's endpoint and wait until each answers
        // before spawning the jail, so a cold/unreachable model fails session start here rather
        // than stalling the main agent's first subagent call. No fallback — the jail is never
        // spawned if warm-up fails. Resume gates separately, in `relaunch_sandboxed_runner`.
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;

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
            BranchIntentPolicy::cursor_cli(),
            project.main_branch_ref.as_deref(),
        )?;
        let mut cs = Changeset {
            workflow: Some(cs_workflow),
            orchestrator_session_id: stack_parent.map(str::to_string),
            recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
            ..Changeset::default()
        };
        if let Some(recipe) = &managed_recipe {
            tddy_core::changeset::update_state(
                &mut cs,
                tddy_core::workflow::ids::WorkflowState::new(recipe.start_goal().as_str()),
            );
        }
        tddy_core::write_changeset(&session_dir, &cs)
            .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;

        let chain_base_ref = Self::resolve_chain_base_ref_status(
            &sessions_base,
            stack_parent,
            &repo_root,
            new_branch_name,
        )?;
        let worktree_base_ref =
            tddy_core::select_worktree_base_ref(selected_integration_base_ref, chain_base_ref);
        let repo_root_clone = repo_root.clone();
        let session_dir_clone = session_dir.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let worktree_path = spawn_blocking_with_timeout(
            timeout,
            "start_sandboxed_cursor_cli_session: create worktree",
            move || {
                tddy_core::setup_worktree_for_session_with_optional_chain_base(
                    &repo_root_clone,
                    &session_dir_clone,
                    worktree_base_ref.as_deref(),
                )
                .map_err(|e| anyhow::anyhow!("worktree setup failed: {e}"))
            },
        )
        .await?;

        push_new_branch_to_origin_if_requested(
            create_remote_branch,
            intent,
            &session_dir,
            &worktree_path,
            timeout,
        )
        .await?;

        let hook_token = crate::cursor_cli_spawn::install_cursor_hooks_in_worktree(
            &self.config,
            &worktree_path,
            session_id,
            os_user,
        );

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
        let scratch_tmp = scratch_dir.join("tmp");
        let context_dir = sandbox_root.join("context");

        // As on the sandboxed claude-cli path: the roster the session starts with is what the
        // withdrawal is computed from, and it is minted before the jail exists.
        let started_agents = started_roster(
            &specialized_defs,
            &local_instance_id_for_config(&self.config),
        )?;
        let replacement_pairs = roster_replacement_pairs(&started_agents);
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
            crate::config::resolve_cursor_cli_tddy_tools_path(&self.config).as_deref(),
        );

        let mut managed: Option<crate::session_toolcall::ManagedWorkflow> = None;
        let mut session_env: Vec<(String, String)> = Vec::new();
        if let Some(recipe) = managed_recipe.clone() {
            let launch = self.prepare_managed_workflow(
                session_id,
                recipe,
                &session_dir,
                &worktree_path,
                &context_dir,
                &tddy_tools_path,
                None,
                None,
            )?;
            if let Ok(prompt) = std::fs::read_to_string(&launch.prompt_file) {
                let rules_dir = worktree_path.join(".cursor").join("rules");
                let _ = std::fs::create_dir_all(&rules_dir);
                let _ = std::fs::write(rules_dir.join("tddy-managed-workflow.mdc"), prompt);
            }
            session_env = launch.env;
            managed = Some(launch.workflow);
        }

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
        let cursor_binary =
            canonicalize_exec(&crate::config::resolve_cursor_binary_path(&self.config));
        let cursor_home_dir = crate::config::resolve_cursor_home_dir(&self.config);
        let scratch_home = crate::sandbox_session::prepare_persistent_cursor_home(
            &cursor_home_dir,
            &cursor_binary,
        );

        let tool_ipc_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(session_id);
        let ready_marker = sandbox_root.join("sandbox.ready");
        let profile_path = sandbox_root.join("sandbox.sb");

        let egress_shim_port =
            crate::sandbox_session::pick_free_loopback_port().map_err(Status::internal)?;
        let loopback_allow_ports = vec![egress_shim_port];

        let mut runner_argv = vec![
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
            "--agent-kind".into(),
            "cursor".into(),
            "--agent-binary".into(),
            cursor_binary.clone(),
            "--model".into(),
            model.to_string(),
            "--egress-shim-port".into(),
            egress_shim_port.to_string(),
            "--stdio".into(),
        ];
        let prompt = initial_prompt.trim();
        if !prompt.is_empty() {
            runner_argv.push("--agent-arg".into());
            runner_argv.push(prompt.to_string());
        }

        // TODO: pin a Cursor chat for sandboxed cursor sessions too (`--agent-arg --resume
        // --agent-arg <id>`), the way the unsandboxed path does in `spawn_cursor_cli_session_inner`.
        // Not done here because the jail runs against its own persistent cursor home
        // (`prepare_persistent_cursor_home`), so a chat minted by the host `cursor-agent
        // create-chat` is not necessarily resolvable inside the jail — that needs verifying before
        // an id is pinned. Until then a sandboxed session records no `cursor_chat_id`, and its
        // first resume adopts a fresh chat (see `resume_cursor_cli_session`).

        // Semantic index: index the worktree into the session dir before spawning the jail
        // (blocking; a missing embedder or a failed index aborts the start — no unindexed
        // fallback), and inject `TDDY_SEMANTIC_INDEX_DB` into the jail env so the in-jail
        // `SemanticSearch` tool resolves against the per-session index.
        let mut semantic_index_env_pair: Option<(String, String)> = None;
        if semantic_index {
            let embedder =
                tddy_semantic_index::production_embedder(&self.tddy_data_dir).map_err(|e| {
                    Status::failed_precondition(format!(
                        "semantic index requested but no embedder is available: {e}"
                    ))
                })?;
            crate::semantic_index::run_semantic_index_blocking(
                &worktree_path,
                &session_dir,
                embedder,
                &self.task_registry,
                session_id,
            )
            .await
            .map_err(|e| Status::internal(format!("semantic index failed: {e}")))?;
            semantic_index_env_pair = Some(crate::semantic_index::semantic_index_env(&session_dir));
        }

        let mut env = crate::sandbox_session::build_sandboxed_cursor_runner_env(
            &scratch_home,
            &scratch_tmp,
            session_id,
            &tool_ipc_socket,
            &egress_dir,
        );
        if !specialized_defs.is_empty() {
            env.extend(self.specialized_subagent_env(&specialized_defs)?);
        }
        env.extend(self.jail_daemon_identity_env());
        env.extend(self.lsp_tools_env(&worktree_path));
        env.extend(semantic_index_env_pair);

        // The jail is this daemon's child and the checkout is this daemon's, so a sandboxed session
        // is facilitated here exactly as an unsandboxed one is — the jail changes what the agent can
        // reach, not who hosts its room.
        open_session_room_before_spawning_agent(
            &self.session_room_host(),
            "cursor-cli",
            session_id,
            &worktree_path,
            &session_dir,
        )
        .await?;

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
                mounts: vec![tddy_sandbox::MountSpec::read_write(scratch_home.clone())],
                host_home: None,
                cgroup: self.config.sandbox_cgroup_config(),
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
        let capture = Arc::new(StdMutex::new(TerminalCapture::new()));
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel();

        crate::sandbox_session::dial_and_bridge(
            session_id,
            worktree_path.clone(),
            &mut handle,
            self.task_registry.clone(),
            stdout_tx.clone(),
            Arc::clone(&capture),
            stdin_rx,
            Arc::new(session_env),
            session_dir.clone(),
            self.agent_activity_hub(),
            Arc::new(DaemonRpcHandler {
                conn: self.self_arc(),
            }),
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
                managed_workflow: managed,
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
            session_type: Some("cursor-cli".to_string()),
            model: Some(model.to_string()),
            cursor_chat_id: None,
            activity_status: None,
            hook_token: Some(hook_token),
            sandbox: Some(true),
            agent: None,
            recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
            agents_rev: started_roster_rev(&started_agents),
            agents: started_agents,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
        };
        tddy_core::write_session_metadata(&session_dir, &meta)
            .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;

        log::info!(
            target: "tddy_daemon::connection_service",
            "started sandboxed cursor-cli session {session_id} pid={pid} worktree={}",
            worktree_path.display()
        );

        Ok(Response::new(StartSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
            branch_conflict: None,
        }))
    }

    /// Handle `ResumeSession` for `session_type = "claude-cli"` sessions.
    async fn resume_claude_cli_session(
        &self,
        os_user: &str,
        session_id: &str,
        session_dir: PathBuf,
        meta: tddy_core::SessionMetadata,
        // The caller's token, re-exported to a split session's agent as TDDY_REMOTE_SESSION_TOKEN:
        // the codebase daemon verifies it on every tool call, so the resumed agent needs a live one.
        session_token: &str,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        if meta.sandbox == Some(true) {
            return self
                .resume_sandboxed_claude_cli_session(os_user, session_id, session_dir, meta)
                .await;
        }
        let model = meta.model.clone().unwrap_or_default();

        // A split session has no `repo_path` here and its `TDDY_REMOTE_*` wiring was injected at
        // spawn time, so both are re-derived from the persisted pairing — including a **fresh** join
        // token, since the original is scoped to a lifetime that may well have elapsed while the
        // session was stopped.
        let split = self.resume_split_wiring(&meta, &session_dir, session_id, session_token)?;
        let worktree_path = split
            .as_ref()
            .map(|w| w.context_dir.clone())
            .or_else(|| meta.repo_path.as_ref().map(PathBuf::from))
            .unwrap_or_else(|| session_dir.clone());
        let (split_args, split_env) = match split {
            Some(w) => (w.extra_args, w.env),
            None => (Vec::new(), Vec::new()),
        };

        let manager = Arc::clone(&self.claude_cli_manager);
        let session_id_owned = session_id.to_string();
        let binary_owned = resolve_resume_session_claude_binary(&self.config);

        // Re-wire managed-workflow orchestration when resuming a managed session — metadata records a
        // recipe only for managed sessions. The controller resumes at the goal persisted in
        // changeset.yaml so the workflow continues from where it left off, not from the start goal.
        let mut managed: Option<crate::session_toolcall::ManagedWorkflow> = None;
        let mut append_system_prompt_file: Option<PathBuf> = None;
        let mut env_extra: Vec<(String, String)> = split_env;
        if let Some(recipe_name) = meta.recipe.as_deref().filter(|s| !s.trim().is_empty()) {
            let recipe = tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(recipe_name)
                .map_err(Status::invalid_argument)?;
            let resume_goal = Self::managed_resume_goal(&session_dir, &recipe);
            let tddy_tools_path = crate::sandbox_session::resolve_tddy_tools_path(
                self.config
                    .claude_cli
                    .as_ref()
                    .and_then(|c| c.tddy_tools_path.as_deref()),
            );
            let launch = self.prepare_managed_workflow(
                &session_id_owned,
                recipe,
                &session_dir,
                &worktree_path,
                &session_dir,
                &tddy_tools_path,
                Some(resume_goal),
                None,
            )?;
            append_system_prompt_file = Some(launch.prompt_file);
            env_extra.extend(launch.env);
            managed = Some(launch.workflow);
        }

        let handle = manager
            .resume_with_options(
                &session_id_owned,
                worktree_path,
                &model,
                &binary_owned,
                append_system_prompt_file.as_deref(),
                split_args,
                env_extra,
            )
            .await
            .map_err(|e| Status::internal(format!("failed to relaunch claude-cli: {}", e)))?;

        if let Some(mw) = managed {
            manager.attach_managed_workflow(&session_id_owned, mw).await;
        }

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

    /// Rebuild the remote-tool wiring for a split session being resumed, or `None` for a co-located
    /// one.
    ///
    /// Nothing about a split session's tool transport survives a stop: the env was injected into a
    /// process that has exited, and the join token it carried is scoped to a TTL that may have
    /// elapsed. Both are minted afresh here from the persisted pairing, which is the only part that
    /// is durable.
    fn resume_split_wiring(
        &self,
        meta: &tddy_core::SessionMetadata,
        session_dir: &Path,
        session_id: &str,
        session_token: &str,
    ) -> Result<Option<crate::split_session::SplitAgentWiring>, Status> {
        let Some((codebase_daemon, codebase_session)) = crate::split_session::split_pairing(meta)
        else {
            return Ok(None);
        };

        let wiring = crate::split_session::prepare_split_agent_wiring(
            &self.config,
            session_dir,
            &self.resolve_tddy_tools_path().to_string_lossy(),
            session_id,
            codebase_daemon,
            codebase_session,
            session_token,
        )?;
        log::info!(
            "ResumeSession: re-wired split session {session_id} to workspace session {codebase_session} on daemon {codebase_daemon}"
        );
        Ok(Some(wiring))
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

        // A recipe in metadata marks a managed session; re-wire its workflow on resume.
        let managed_recipe = match meta.recipe.as_deref().filter(|s| !s.trim().is_empty()) {
            Some(name) => Some(
                tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(name)
                    .map_err(Status::invalid_argument)?,
            ),
            None => None,
        };

        let pid = self
            .relaunch_sandboxed_runner(
                session_id,
                &session_dir,
                &worktree_path,
                &model,
                "auto",
                &meta.agents,
                managed_recipe,
                // Resume path: the transcript already exists under the persistent sandbox claude
                // HOME, so the runner must launch `claude --resume <id>`, not `--session-id <id>`.
                true,
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
        // The session's **persisted** roster, not the names its start request carried: an agent
        // attached while the session ran is in the former and in neither the latter nor the jail's
        // previous seed, and a relaunch that read the request would hand the main agent back a tool
        // the operator had withdrawn from it (PRD AC25).
        agents: &[tddy_core::SessionAgentRecord],
        managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>>,
        // When true, spawn the runner with `--resume` so the jailed `claude` continues the existing
        // on-disk transcript (`--resume <id>`) instead of assigning the id to a fresh session
        // (`--session-id <id>`). The persistent sandbox claude HOME keeps the transcript across
        // daemon restarts, so a fresh `--session-id` would abort with "Session ID already in use".
        resume: bool,
    ) -> Result<u32, Status> {
        // The defs are re-resolved for what the jail's *seed* and the warm-up need — an endpoint to
        // wake and a registry to start from. What the main agent loses comes from the roster below,
        // never from these: a def edited since the attach must not change a running session's tools.
        let specialized_defs = self
            .resolve_specialized_agent_defs(&roster_agent_ids(agents))
            .await?;

        // The same readiness gate the start paths apply: a resumed session's subagents are only as
        // usable as a fresh one's if their endpoints are awake before the jail comes back up.
        // Without this, resume would hand the agent a subagent whose first call stalls on a cold
        // model. No fallback — the runner is not relaunched if warm-up fails.
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;

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
        // scratch_home (jail $HOME) is the persistent daemon-wide claude home, resolved and mounted
        // below — not a per-session dir — so auth/history persist across sessions.
        let scratch_tmp = scratch_dir.join("tmp");
        let context_dir = sandbox_root.join("context");

        let replacement_pairs = roster_replacement_pairs(agents);
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

        // Re-wire managed-workflow orchestration on resume of a managed sandboxed session; the
        // controller resumes at the goal persisted in changeset.yaml. The prompt goes into the
        // jail-visible context dir and the per-session env carries TDDY_SOCKET for host-side
        // `tddy-tools transition` (relayed via the Shell tool).
        let mut managed: Option<crate::session_toolcall::ManagedWorkflow> = None;
        let mut append_system_prompt_file: Option<PathBuf> = None;
        let mut session_env: Vec<(String, String)> = Vec::new();
        if let Some(recipe) = managed_recipe {
            let resume_goal = Self::managed_resume_goal(session_dir, &recipe);
            let launch = self.prepare_managed_workflow(
                session_id,
                recipe,
                session_dir,
                worktree_path,
                &context_dir,
                &tddy_tools_path,
                Some(resume_goal),
                None,
            )?;
            append_system_prompt_file = Some(launch.prompt_file);
            session_env = launch.env;
            managed = Some(launch.workflow);
        }

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
        // See the sibling call site above: resolve the real `claude` (overridable via
        // TDDY_CLAUDE_BINARY / `claude_cli.binary_path`); a bare name breaks the sandbox profile.
        let claude_binary = crate::config::resolve_claude_binary_path(&self.config);

        // Persistent daemon-wide jail $HOME (see sibling site above): mounted read-write, seeded
        // non-clobbering, so auth/history persist across sessions.
        let claude_home_dir = crate::config::resolve_claude_home_dir(&self.config);
        let scratch_home = crate::sandbox_session::prepare_persistent_claude_home(
            &claude_home_dir,
            &claude_binary,
        );

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

        let mut runner_argv = vec![
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
        if resume {
            runner_argv.push("--resume".into());
        }
        if let Some(prompt_path) = &append_system_prompt_file {
            runner_argv.push("--append-system-prompt-file".into());
            runner_argv.push(prompt_path.to_string_lossy().to_string());
        }

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
        env.extend(self.jail_daemon_identity_env());
        env.extend(self.lsp_tools_env(worktree_path));

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
                // Mount the persistent jail $HOME read-write so it survives the session.
                mounts: vec![tddy_sandbox::MountSpec::read_write(scratch_home.clone())],
                // Persistent home is seeded separately (non-clobbering); disable the recipe's
                // per-session credential copy so it can't overwrite a refreshed jail token.
                host_home: None,
                cgroup: self.config.sandbox_cgroup_config(),
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
        let capture = Arc::new(StdMutex::new(TerminalCapture::new()));
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel();

        crate::sandbox_session::dial_and_bridge(
            session_id,
            worktree_path.to_path_buf(),
            &mut handle,
            self.task_registry.clone(),
            stdout_tx.clone(),
            Arc::clone(&capture),
            stdin_rx,
            Arc::new(session_env),
            session_dir.to_path_buf(),
            self.agent_activity_hub(),
            Arc::new(DaemonRpcHandler {
                conn: self.self_arc(),
            }),
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
                managed_workflow: managed,
            },
        ));
        self.sandbox_manager
            .insert(session_id.to_string(), state)
            .await;
        Ok(pid)
    }
}

/// Launch inputs for a managed claude-cli session, produced by
/// [`ConnectionServiceImpl::prepare_managed_workflow`]: the workflow wiring (whose listener must be
/// kept alive for the session's lifetime), the orchestration-prompt file to append to claude's
/// system prompt, and the per-session env (`TDDY_SOCKET` + `PATH`) for host-side `tddy-tools`.
struct ManagedLaunch {
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
    /// Opens the session room of each child agent this handler spawns — the children are
    /// agent sessions of this same daemon, so it facilitates their rooms too.
    room_host: Arc<dyn crate::session_room::SessionRoomHost>,
    config: DaemonConfig,
    tddy_data_dir: PathBuf,
    claude_cli_manager: Arc<CliSessionManager>,
    os_user: String,
    project_id: String,
    sessions_base: PathBuf,
    orchestrator_session_id: String,
    orchestrator_session_dir: PathBuf,
}

#[async_trait::async_trait]
impl tddy_core::toolcall::ChildSpawnHandler for StackChildSpawnHandler {
    async fn spawn_child(&self, node_id: &str) -> Result<String, String> {
        let changeset = tddy_core::read_changeset(&self.orchestrator_session_dir)
            .map_err(|e| format!("failed to read orchestrator changeset: {e}"))?;
        let stack = changeset
            .stack
            .ok_or_else(|| "orchestrator changeset has no stack".to_string())?;
        let node = stack
            .nodes
            .iter()
            .find(|n| n.node_id == node_id)
            .ok_or_else(|| format!("no planned PR node with id '{node_id}' in the stack"))?;
        // "Already spawned" means the node owns a branch: that branch is the work, and it outlives
        // whichever session created it. Spawning again would try to create a branch that exists.
        if let Some(branch) = node.branch.as_deref() {
            return Err(format!("node '{node_id}' already owns branch '{branch}'"));
        }
        let new_branch_name = node
            .branch_suggestion
            .clone()
            .ok_or_else(|| format!("node '{node_id}' has no branch_suggestion to create"))?;
        let initial_prompt = if node.description.trim().is_empty() {
            node.title.clone()
        } else {
            format!("{}\n\n{}", node.title, node.description)
        };
        // Inherit the orchestrator's model — the daemon has no standalone model default and an
        // empty model is rejected by the spawn path.
        let meta = tddy_core::read_session_metadata(&self.orchestrator_session_dir)
            .map_err(|e| format!("failed to read orchestrator session metadata: {e}"))?;
        let model = meta.model.clone().unwrap_or_default();
        if model.trim().is_empty() {
            return Err("orchestrator session has no model to inherit for the child".to_string());
        }

        let child_session_id = Uuid::new_v4().to_string();
        let response = spawn_claude_cli_session_inner(
            &self.config,
            &self.tddy_data_dir,
            &self.claude_cli_manager,
            &self.os_user,
            &child_session_id,
            self.sessions_base.clone(),
            &model,
            &self.project_id,
            "new_branch_from_base",
            &new_branch_name,
            "",
            "",
            &initial_prompt,
            "auto",
            false,
            Some(&self.orchestrator_session_id),
            None,
            None,
            None,
            // A spawned child session never runs its own semantic index.
            false,
            // Child spawns are created by the orchestrator agent, not the Start-Session dialog, and
            // never push a remote branch here.
            false,
            &self.claude_cli_manager.task_registry(),
            self.room_host.as_ref(),
        )
        .await
        .map_err(|status| status.message().to_string())?;
        Ok(response.into_inner().session_id)
    }
}

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
    /// Opens the session room of each child agent this handler spawns — the children are
    /// agent sessions of this same daemon, so it facilitates their rooms too.
    room_host: Arc<dyn crate::session_room::SessionRoomHost>,
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

#[async_trait::async_trait]
impl tddy_core::toolcall::ConversationSpawnHandler for GrillMeConversationSpawnHandler {
    async fn spawn_conversation(
        &self,
        prompt: &str,
        branch: Option<&str>,
        base_ref: Option<&str>,
    ) -> Result<String, String> {
        if prompt.trim().is_empty() {
            return Err("spawn_conversation requires a non-empty prompt".to_string());
        }
        let new_branch_name = branch
            .map(str::to_string)
            .filter(|b| !b.trim().is_empty())
            .unwrap_or_else(|| conversation_branch_slug(prompt));

        // Inherit the orchestrator's model — the daemon has no standalone model default and an
        // empty model is rejected by the spawn path.
        let meta = tddy_core::read_session_metadata(&self.orchestrator_session_dir)
            .map_err(|e| format!("failed to read orchestrator session metadata: {e}"))?;
        let model = meta
            .model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .or_else(|| self.model_override.clone())
            .unwrap_or_default();
        if model.trim().is_empty() {
            return Err(
                "orchestrator session has no model to inherit for the conversation".to_string(),
            );
        }

        let child_session_id = Uuid::new_v4().to_string();
        let response = spawn_claude_cli_session_inner(
            &self.config,
            &self.tddy_data_dir,
            &self.claude_cli_manager,
            &self.os_user,
            &child_session_id,
            self.sessions_base.clone(),
            &model,
            &self.project_id,
            "new_branch_from_base",
            &new_branch_name,
            base_ref.unwrap_or_default(),
            "",
            prompt,
            "auto",
            false,
            Some(&self.orchestrator_session_id),
            None,
            None,
            None,
            // A spawned child conversation never runs its own semantic index.
            false,
            // Child conversations are spawned by the orchestrator, never pushing a remote branch.
            false,
            &self.claude_cli_manager.task_registry(),
            self.room_host.as_ref(),
        )
        .await
        .map_err(|status| status.message().to_string())?;
        Ok(response.into_inner().session_id)
    }
}

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

fn file_mtime_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn cleanup_materialized_attachments(session_dir: &Path, written_basenames: &[String]) {
    let attachments_dir = tddy_workflow::session_attachments_root(session_dir);
    for basename in written_basenames {
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
struct AttachmentProgressSink {
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
struct AttachmentProgressReporter<'a> {
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
struct AttachmentMaterialization<'a> {
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
enum SplitStartFailure {
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

impl ConnectionServiceImpl {
    fn resolve_os_user(&self, session_token: &str) -> Result<String, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        self.config
            .os_user_for_github(&github_user)
            .map(|s| s.to_string())
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))
    }

    fn eligible_instance_ids(&self) -> Vec<String> {
        self.eligible_daemon_source
            .list_eligible_daemons()
            .into_iter()
            .map(|e| e.instance_id.0)
            .collect()
    }

    fn classify_daemon_route(&self, requested_daemon: &str) -> Result<PeerRoute, Status> {
        let local_id = local_instance_id_for_config(&self.config);
        crate::livekit_peer_discovery::classify_peer_route(
            &local_id,
            requested_daemon,
            &self.eligible_instance_ids(),
        )
        .map_err(|msg| {
            log::info!("daemon routing rejected: {msg}");
            Status::failed_precondition(msg)
        })
    }

    /// Route an exec-tool RPC by its requested `daemon_instance_id`, before any session lookup — a
    /// relay holds no sessions of its own and must still be able to forward.
    ///
    /// Unlike [`Self::classify_daemon_route`], an unroutable id is `InvalidArgument`: the caller
    /// named a daemon that cannot serve the call, which is a bad request rather than a deployment
    /// that is not ready. Shared by the unary and streaming handlers so they cannot diverge.
    fn classify_exec_tool_route(
        &self,
        rpc_name: &str,
        requested_daemon: &str,
    ) -> Result<PeerRoute, Status> {
        let requested_daemon = requested_daemon.trim();
        if requested_daemon.is_empty() {
            return Ok(PeerRoute::Local);
        }
        let route = crate::livekit_peer_discovery::classify_peer_route(
            &local_instance_id_for_config(&self.config),
            requested_daemon,
            &self.eligible_instance_ids(),
        )
        .map_err(|msg| {
            log::info!("{rpc_name}: rejected daemon routing: {msg}");
            Status::invalid_argument(msg)
        })?;
        if let PeerRoute::Forward { peer_instance_id } = &route {
            log::info!(
                "{rpc_name}: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
        }
        Ok(route)
    }

    /// Authenticate an exec-tool caller, and answer with the OS user its tools run as here.
    ///
    /// Separate from [`Self::resolve_exec_tool_worktree`] because it has to run **before** the
    /// hosted-clone branch, which resolves no worktree of this daemon's at all and whose mutating
    /// half proxies to the facilitating daemon under the *clone's* stored credential. Reached with
    /// no check of its own, that branch would let any common-room participant that read a session id
    /// out of a `session.agents` broadcast land an arbitrary write in another host's authoritative
    /// worktree.
    ///
    /// Both refusals name **this** daemon. For a split session the tools are served on the codebase
    /// host while the error is rendered in the agent's transcript on the agent host, where an
    /// unattributed "invalid or expired session" reads as the agent host's own answer — and the two
    /// likeliest split misconfigurations land here: daemons not sharing `livekit.api_secret` (a
    /// session token is a stateless HMAC, verifiable only by daemons holding the same secret), and a
    /// GitHub user mapped on the agent host but not on the codebase host. Each is also logged here,
    /// because the operator debugging it is reading *this* daemon's log.
    fn authorize_exec_tool_caller(&self, req: &ExecuteToolRequest) -> Result<&str, Status> {
        let local_instance_id = local_instance_id_for_config(&self.config);
        let Some(github_user) = (self.user_resolver)(&req.session_token) else {
            log::warn!(
                "exec tool {tool:?} for session {session} refused on daemon {local_instance_id}: the session token could not be verified here (a split session's agent presents a token minted by its agent daemon, so both daemons must share livekit.api_secret)",
                tool = req.tool_name,
                session = req.session_id
            );
            return Err(Status::unauthenticated(format!(
                "daemon {local_instance_id} could not verify the session token (invalid or expired there); a split session's tools run on the daemon holding the codebase, which verifies the token with its own livekit.api_secret"
            )));
        };
        let Some(os_user) = self.config.os_user_for_github(&github_user) else {
            log::warn!(
                "exec tool {tool:?} for session {session} refused on daemon {local_instance_id}: GitHub user {github_user} has no users[] entry here",
                tool = req.tool_name,
                session = req.session_id
            );
            return Err(Status::permission_denied(format!(
                "daemon {local_instance_id} has no OS user mapped for GitHub user {github_user}; add a users[] entry there — a split session's tools run as that user on the daemon holding the codebase"
            )));
        };
        Ok(os_user)
    }

    /// Resolve, on this daemon, the sessions base and the worktree an exec tool runs in — for a
    /// caller [`Self::authorize_exec_tool_caller`] has already accepted.
    fn resolve_exec_tool_worktree(
        &self,
        req: &ExecuteToolRequest,
    ) -> Result<(PathBuf, PathBuf), Status> {
        let os_user = self.authorize_exec_tool_caller(req)?;

        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let worktree_root =
            workspace_session::resolve_worktree_root_for_session(&sessions_base, &req.session_id)?;
        Ok((sessions_base, worktree_root))
    }

    /// Run one tool call in `worktree_root` and durably record it.
    ///
    /// A tool failure is carried in the returned response, never raised as an RPC error: only
    /// routing and auth failures are RPC errors, so an agent can tell "the tool said no" from "the
    /// call never reached the tool".
    async fn run_exec_tool_locally(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> ExecuteToolResponse {
        let outcome = tool_engine::execute_tool(
            worktree_root,
            &req.tool_name,
            &req.args_json,
            &self.task_registry,
            &req.session_id,
        )
        .await;

        // Durably record the tool call (non-fatal on failure).
        let session_dir = unified_session_dir_path(sessions_base, &req.session_id);
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

        ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        }
    }

    fn common_room_slot(
        &self,
        rpc_name: &str,
    ) -> Result<&Arc<tokio::sync::RwLock<Option<Arc<Room>>>>, Status> {
        self.common_room_livekit_room.as_ref().ok_or_else(|| {
            Status::failed_precondition(format!(
                "cannot forward {rpc_name}: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)"
            ))
        })
    }

    /// Pre-creates `session_dir` when needed and materializes the request's attachments before spawn.
    async fn prepare_session_attachments(
        &self,
        ctx: &AttachmentMaterialization<'_>,
    ) -> Result<(), Status> {
        if ctx.attachments.is_empty() {
            return Ok(());
        }
        let session_dir = ctx.session_dir();
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;
        self.materialize_session_attachments(ctx).await
    }

    async fn materialize_session_attachments(
        &self,
        ctx: &AttachmentMaterialization<'_>,
    ) -> Result<(), Status> {
        if ctx.attachments.is_empty() {
            return Ok(());
        }

        let session_dir = ctx.session_dir();
        let local_instance_id = local_instance_id_for_config(&self.config);
        let mut seen_basenames = std::collections::HashSet::new();
        for att in ctx.attachments {
            let safe = validate_attachment_basename(&att.basename)?;
            if !seen_basenames.insert(safe.to_string()) {
                return Err(Status::invalid_argument(
                    "duplicate attachment basename in request",
                ));
            }
        }

        let staging_root = crate::session_attachment_staging::staging_root_for(
            ctx.os_user,
            &self.staging_base_dir,
        );
        let mut written: Vec<String> = Vec::new();
        let attachment_count = ctx.attachments.len() as u32;

        for (index, att) in ctx.attachments.iter().enumerate() {
            let basename = validate_attachment_basename(&att.basename)?.to_string();
            let source = att
                .source
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("attachment source must be set"))?;
            let reporter = AttachmentProgressReporter {
                sink: ctx.progress,
                basename: &basename,
                attachment_index: index as u32,
                attachment_count,
            };

            let materialize_result = match source {
                AttachmentSource::Staged(staged) => {
                    self.materialize_staged_attachment(
                        ctx.session_token,
                        &staging_root,
                        &session_dir,
                        staged,
                        &basename,
                        &reporter,
                    )
                    .await
                }
                AttachmentSource::HostDocument(host_doc) => {
                    self.materialize_host_document_attachment(
                        ctx.session_token,
                        ctx.os_user,
                        &session_dir,
                        host_doc,
                        &basename,
                        &local_instance_id,
                    )
                    .await
                }
            };

            match materialize_result {
                Ok(()) => {
                    // The attachment is on disk now, so its final size is the honest byte count to
                    // report — and it is the only report a source that copies in one step makes.
                    let bytes = attachment_size_bytes(&session_dir, &basename);
                    reporter.report(bytes, bytes);
                    written.push(basename);
                }
                Err(e) => {
                    cleanup_materialized_attachments(&session_dir, &written);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Copies one staged file into the session's attachments.
    ///
    /// The browser stages to whichever daemon it is connected to and may then start the session on
    /// another host, so a ref naming a foreign daemon is fetched from that daemon through the
    /// `STAGED_ATTACHMENT` host-document scope — which applies the containment and
    /// completeness-marker guards on the **owning** side, the only host that can tell a truncated
    /// upload from a whole one. That fetch goes over the *streaming* read
    /// ([`Self::fetch_peer_staged_attachment`]), so crossing hosts does not shrink the size a
    /// session will accept. A local ref is copied straight off disk: there is no reason to
    /// round-trip bytes through RPC on a single host.
    async fn materialize_staged_attachment(
        &self,
        session_token: &str,
        staging_root: &Path,
        session_dir: &Path,
        staged: &StagedAttachmentRef,
        basename: &str,
        progress: &AttachmentProgressReporter<'_>,
    ) -> Result<(), Status> {
        let safe_staging = validate_segment(&staged.staging_id)?;
        let safe_name = validate_segment(&staged.file_name)?;

        match self.classify_daemon_route(&staged.daemon_instance_id)? {
            PeerRoute::Local => Self::copy_local_staged_attachment(
                staging_root,
                session_dir,
                safe_staging,
                safe_name,
                basename,
            ),
            PeerRoute::Forward { peer_instance_id } => {
                self.fetch_peer_staged_attachment(
                    session_token,
                    session_dir,
                    &peer_instance_id,
                    &format!("{safe_staging}/{safe_name}"),
                    basename,
                    progress,
                )
                .await
            }
        }
    }

    /// Copies a staged file that already lives on this host into the session's attachments.
    fn copy_local_staged_attachment(
        staging_root: &Path,
        session_dir: &Path,
        safe_staging: &str,
        safe_name: &str,
        basename: &str,
    ) -> Result<(), Status> {
        let batch_dir = staging_root.join(safe_staging);
        if !batch_dir.exists() {
            return Err(Status::invalid_argument("staged attachment file not found"));
        }
        let canonical_dir = contained_canonical_dir(staging_root, &batch_dir)?;
        let staged_path = canonical_dir.join(safe_name);
        if !staged_path.is_file() {
            return Err(Status::invalid_argument("staged attachment file not found"));
        }
        // The writer only marks a staged file complete on its final chunk; refuse an
        // in-progress or aborted upload so the agent never sees truncated bytes.
        if !crate::session_attachment_staging::staged_complete_marker(&canonical_dir, safe_name)
            .exists()
        {
            return Err(Status::failed_precondition(
                "staged attachment upload is not complete",
            ));
        }

        crate::session_attachments::copy_attachment_into_session(
            session_dir,
            &staged_path,
            basename,
        )?;
        Ok(())
    }

    /// Fetches a staged file from the peer that owns it, over the **streaming** host-document read,
    /// reporting each frame's arrival as progress.
    ///
    /// The unary read carries its own `MAX_HOST_DOCUMENT_BYTES` ceiling — a transport message-size
    /// budget, not a policy. Routing a cross-host staged ref through it would refuse a document
    /// that materializes fine when the session runs on the staging host, so the same attachment
    /// would succeed on one host and fail across two. The stream has no per-message ceiling, which
    /// leaves the host's configured `max_attachment_bytes` as the single limit on both paths.
    ///
    /// This is the slowest thing a start-session request does, and on the feature's primary flow —
    /// bytes staged on the host the browser is connected to, session started on another — it is the
    /// *only* thing between accepting the request and reporting the first byte of work. Reporting
    /// per frame is therefore what keeps a relayed `StreamStartSession` producing inside
    /// [`crate::livekit_peer_discovery::PEER_FORWARD_STREAM_IDLE_TIMEOUT`], and what makes the row's
    /// progress bar advance instead of sitting at 0% for the whole transfer.
    async fn fetch_peer_staged_attachment(
        &self,
        session_token: &str,
        session_dir: &Path,
        peer_instance_id: &str,
        staged_relative_path: &str,
        basename: &str,
        progress: &AttachmentProgressReporter<'_>,
    ) -> Result<(), Status> {
        let slot = self.common_room_slot("StreamReadHostDocument")?;
        let read_req = ReadHostDocumentRequest {
            session_token: session_token.to_string(),
            daemon_instance_id: peer_instance_id.to_string(),
            scope: HostDocumentScope::StagedAttachment.into(),
            session_id: String::new(),
            project_id: String::new(),
            relative_path: staged_relative_path.to_string(),
        };
        let mut frames =
            crate::livekit_peer_discovery::forward_stream_read_host_document_via_livekit(
                slot,
                peer_instance_id,
                &read_req,
            )
            .await?;

        // The owning host refuses an over-cap document before its first frame, but a forwarded
        // stream is bytes from a peer — hold the same configured cap here, and stop as soon as it
        // is crossed rather than buffering an unbounded document into memory.
        let max_bytes = self.config.max_attachment_bytes;
        let mut data: Vec<u8> = Vec::new();
        while let Some(frame) = frames.recv().await {
            let frame = frame?;
            data.extend_from_slice(&frame.data);
            if data.len() as u64 > max_bytes {
                return Err(Status::invalid_argument(format!(
                    "staged attachment exceeds this host's maximum attachment size of {max_bytes} bytes"
                )));
            }
            // The peer stamps the whole document's size on every frame, so each one is a complete
            // progress reading with no preamble needed.
            progress.report(data.len() as u64, frame.total_byte_size);
        }

        crate::session_attachments::write_attachment_bytes(session_dir, basename, &data)?;
        Ok(())
    }

    async fn materialize_host_document_attachment(
        &self,
        session_token: &str,
        os_user: &str,
        session_dir: &Path,
        host_doc: &HostDocumentRef,
        basename: &str,
        local_instance_id: &str,
    ) -> Result<(), Status> {
        let scope =
            HostDocumentScope::try_from(host_doc.scope).unwrap_or(HostDocumentScope::Unspecified);
        let ref_daemon = host_doc.daemon_instance_id.trim();

        let bytes = if ref_daemon.is_empty() || ref_daemon == local_instance_id {
            crate::host_documents::read_host_document_bytes(
                os_user,
                &self.tddy_data_dir,
                &self.staging_base_dir,
                scope,
                &host_doc.session_id,
                &host_doc.project_id,
                &host_doc.relative_path,
            )?
        } else {
            let route = self.classify_daemon_route(ref_daemon)?;
            match route {
                PeerRoute::Local => crate::host_documents::read_host_document_bytes(
                    os_user,
                    &self.tddy_data_dir,
                    &self.staging_base_dir,
                    scope,
                    &host_doc.session_id,
                    &host_doc.project_id,
                    &host_doc.relative_path,
                )?,
                PeerRoute::Forward { peer_instance_id } => {
                    let slot = self.common_room_slot("ReadHostDocument")?;
                    let read_req = ReadHostDocumentRequest {
                        session_token: session_token.to_string(),
                        daemon_instance_id: ref_daemon.to_string(),
                        scope: host_doc.scope,
                        session_id: host_doc.session_id.clone(),
                        project_id: host_doc.project_id.clone(),
                        relative_path: host_doc.relative_path.clone(),
                    };
                    let resp =
                        crate::livekit_peer_discovery::forward_read_host_document_via_livekit(
                            slot,
                            &peer_instance_id,
                            &read_req,
                        )
                        .await?;
                    crate::host_documents::HostDocumentBytes {
                        data: resp.data,
                        byte_size: resp.byte_size,
                    }
                }
            }
        };

        // Defense in depth: the owning daemon enforces `MAX_HOST_DOCUMENT_BYTES` on a local
        // read, but a forwarded response is trusted bytes from a peer — re-check the cap on
        // the session host before writing, so a buggy/older peer cannot push an oversized
        // blob into the session's attachments.
        if bytes.data.len() > crate::host_documents::MAX_HOST_DOCUMENT_BYTES {
            return Err(Status::invalid_argument(format!(
                "host document exceeds maximum size of {} bytes",
                crate::host_documents::MAX_HOST_DOCUMENT_BYTES
            )));
        }

        crate::session_attachments::write_attachment_bytes(session_dir, basename, &bytes.data)?;
        Ok(())
    }

    /// Start a **split** session: the agent runs here, its worktree lives on `codebase_instance_id`.
    ///
    /// The codebase daemon creates a `workspace` session holding the worktree; this daemon spawns the
    /// agent with no repository on disk and wires it to that worktree through `mcp__tddy-tools__*`
    /// over LiveKit (`docs/ft/daemon/remote-managed-worktree.md`).
    ///
    /// Atomic by construction: everything that can be resolved locally is resolved *before* the peer
    /// is asked to create anything, and any failure after it has done so tears its session back down.
    /// A half-built split session would strand a worktree on a host with no session left to reclaim
    /// it.
    async fn start_split_claude_cli_session(
        &self,
        os_user: &str,
        codebase_instance_id: &str,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        // These three ask for work that only exists where the repository is. Refused rather than
        // silently dropped, because a session that came up without its recipe (or its index) looks
        // like the session that was asked for.
        if !req.recipe.trim().is_empty() {
            return Err(Status::invalid_argument(
                "a workflow recipe needs a repository on the daemon running the agent; it cannot be combined with codebase_daemon_instance_id",
            ));
        }
        if req.semantic_index {
            return Err(Status::invalid_argument(
                "semantic_index indexes a worktree on this daemon; it cannot be combined with codebase_daemon_instance_id",
            ));
        }
        if req.sandbox {
            return Err(Status::invalid_argument(
                "sandbox sessions resolve their worktree on this daemon; it cannot be combined with codebase_daemon_instance_id",
            ));
        }

        let slot = self.common_room_slot("StartSession")?.clone();

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_id = Uuid::now_v7().to_string();

        // The workspace session's id is chosen *here*, before the peer is asked for anything, and
        // travels in the request. Letting the peer name it would make the answer the only way to
        // learn the id — so a forward that errors or times out while the peer goes on building
        // would leave a worktree on that host with nothing pointing at it and no way to name it in
        // a teardown. This is what makes the failure atomic rather than merely usually atomic.
        let codebase_session_id = Uuid::now_v7().to_string();

        // Resolved before the peer is contacted: a room this daemon cannot mint a token for means
        // the agent could never reach its checkout, so nothing should be created for it. The room is
        // *this* session's and is hosted here — this daemon runs the agent, so it is the session's
        // facilitating daemon whether or not the repo turns out to live somewhere else.
        let livekit = crate::split_session::SplitLiveKitRoom::from_config(
            &self.config,
            crate::session_room::session_room_name(&session_id),
        )?;

        // The peer runs this locally and holds the codebase for it, so it must not route the request
        // onward: `daemon_instance_id` named *this* host, and a codebase host of its own would make
        // it split the session again.
        // Attachments belong beside the agent, which is here: they are read by the agent and by the
        // browser's Docs listing, both of which act against *this* session on *this* daemon. Sending
        // them on would put a second copy on a host with no reader for it, and pay the transfer
        // inside the forward's deadline to do so.
        let workspace_req = StartSessionRequest {
            session_type: "workspace".to_string(),
            daemon_instance_id: String::new(),
            codebase_daemon_instance_id: String::new(),
            requested_session_id: codebase_session_id.clone(),
            attachments: Vec::new(),
            ..req.clone()
        };
        let forwarded = crate::livekit_peer_discovery::forward_start_session_via_livekit_within(
            &slot,
            codebase_instance_id,
            &workspace_req,
            self.split_forward_deadline(),
        )
        .await;
        let workspace = match forwarded {
            Ok(workspace) => workspace,
            Err(status) => {
                // The peer may have created the session and its worktree, may have failed part-way
                // through, or may never have started — none of which this side can distinguish. The
                // teardown covers all three, because the id was ours to begin with.
                self.tear_down_codebase_session(
                    &slot,
                    codebase_instance_id,
                    &codebase_session_id,
                    &req.session_token,
                    SplitStartFailure::from_forward_error(&status),
                )
                .await;
                return Err(status);
            }
        };
        // A branch another session owns is reported, not created: the peer built nothing, so the
        // conflict travels back to the caller as it would for a co-located start.
        if workspace.branch_conflict.is_some() {
            return Ok(Response::new(workspace));
        }
        let created_session_id = workspace.session_id.trim();
        if created_session_id.is_empty() {
            return Err(Status::internal(format!(
                "daemon {codebase_instance_id} answered StartSession with no session id; the worktree's placement cannot be recorded"
            )));
        }
        if created_session_id != codebase_session_id {
            // A peer that ignored `requested_session_id` cannot give the guarantee above: the next
            // forward it serves slowly would orphan its worktree. Refused rather than accepted with
            // a warning, and the session it did create is torn down under the id it reported.
            self.tear_down_codebase_session(
                &slot,
                codebase_instance_id,
                created_session_id,
                &req.session_token,
                SplitStartFailure::PeerAnswered,
            )
            .await;
            return Err(Status::internal(format!(
                "daemon {codebase_instance_id} created workspace session {created_session_id:?} instead of the requested {codebase_session_id:?}; it does not honour requested_session_id, so a split session's worktree could not be reclaimed after a failed start"
            )));
        }
        // Nothing about the peer's LiveKit fields is checked here any more: a codebase daemon hosts
        // no room. It holds a checkout and answers `GetWorktreeSnapshot` and tool calls about it,
        // both of which this daemon reaches over the peer routing it already uses.
        let started = self
            .spawn_split_agent(
                os_user,
                &session_id,
                &sessions_base,
                codebase_instance_id,
                &codebase_session_id,
                &livekit,
                req,
                progress,
            )
            .await;

        match started {
            Ok(response) => Ok(response),
            Err(status) => {
                // The agent spawn is this daemon's own work: the peer already answered, and
                // whatever it built is there to be reclaimed.
                self.tear_down_codebase_session(
                    &slot,
                    codebase_instance_id,
                    &codebase_session_id,
                    &req.session_token,
                    SplitStartFailure::PeerAnswered,
                )
                .await;
                Err(status)
            }
        }
    }

    /// Spawn the agent half of a split session and record the pairing.
    #[allow(clippy::too_many_arguments)]
    async fn spawn_split_agent(
        &self,
        os_user: &str,
        session_id: &str,
        sessions_base: &Path,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        livekit: &crate::split_session::SplitLiveKitRoom,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;
        self.prepare_session_attachments(&AttachmentMaterialization {
            session_token: &req.session_token,
            os_user,
            sessions_base,
            session_id,
            attachments: &req.attachments,
            progress,
        })
        .await?;

        let tddy_tools_path = self.resolve_tddy_tools_path();
        let remote = crate::split_session::split_remote_tool_env(
            livekit,
            session_id,
            codebase_instance_id,
            codebase_session_id,
            &req.session_token,
        )?;
        // This daemon runs the agent, so it is this session's facilitating daemon and hosts its room —
        // even though the checkout is on `codebase_instance_id`. Opened before the agent is spawned
        // (PRD FR2), and measured by asking the codebase daemon rather than by reading a filesystem
        // this host does not have (FR5). The agent's token was minted for exactly this room.
        //
        // The poller signs its own credential per poll under the verified caller's identity rather
        // than re-presenting `req.session_token`, which the codebase daemon stops accepting five
        // minutes in — see `RoomPollTokenMinter`.
        let token_minter = Arc::new(crate::split_session::RoomPollTokenMinter::new(
            &livekit.api_secret,
            &req.session_token,
        )?);
        let remote_source = Arc::new(crate::session_room::RemoteCheckout::new(
            Arc::new(self.clone()),
            codebase_session_id.to_string(),
            codebase_instance_id.to_string(),
            token_minter,
            session_dir.clone(),
        ));
        let local_instance_id = local_instance_id_for_config(&self.config);
        match self
            .session_rooms
            .open_measured_by(
                &crate::session_room::DaemonRoomHosting {
                    config: &self.config,
                    instance_id: &local_instance_id,
                    rooms: &self.session_rooms,
                }
                .for_remote_worktree(session_id, &session_dir),
                tddy_service::ConnectionServiceServer::new(self.clone()),
                remote_source,
            )
            .await?
        {
            Some(room) => log::info!(
                "split session {session_id} facilitated in {} as {}, measuring session {codebase_session_id} on daemon {codebase_instance_id}",
                room.room,
                room.server_identity
            ),
            None => log::debug!(
                "split session {session_id} runs without a session room (LiveKit not configured)"
            ),
        }

        let context_dir = crate::split_session::build_split_context_dir(&session_dir)?;
        let extra_args = crate::split_session::split_claude_extra_args(
            &session_dir,
            &tddy_tools_path.to_string_lossy(),
        )?;

        // Claude Code reads `.claude/settings.local.json` from its working directory, which for a
        // split session is the context dir rather than a worktree. Best-effort, as elsewhere: a
        // missing hook file costs status reporting, not the session.
        let hook_token = Uuid::new_v4().to_string();
        write_claude_hooks_settings(
            &context_dir,
            &tddy_core::HookCommandParams {
                tddy_tools_path: &tddy_tools_path.to_string_lossy(),
                daemon_url: &claude_hook_daemon_url(&self.config),
                session_id,
                os_user,
                hook_token: &hook_token,
            },
        );

        let handle = self
            .claude_cli_manager
            .start_with_options(
                session_id,
                context_dir,
                req.model.trim(),
                &resolve_start_session_claude_binary(&self.config),
                Some(req.initial_prompt.trim()).filter(|p| !p.is_empty()),
                Some(req.permission_mode.trim()).filter(|m| !m.is_empty()),
                req.dangerously_skip_permissions,
                false,
                None,
                extra_args,
                remote.env_pairs(),
                Some(os_user),
            )
            .await
            .map_err(|e| Status::internal(format!("failed to spawn claude-cli: {e}")))?;

        let now = chrono::Utc::now().to_rfc3339();
        let meta = tddy_core::SessionMetadata {
            session_id: session_id.to_string(),
            project_id: req.project_id.trim().to_string(),
            created_at: now.clone(),
            updated_at: now,
            status: "active".to_string(),
            // No repository on this host — the pairing below is how the worktree is found.
            repo_path: None,
            pid: Some(handle.pid),
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some(req.model.trim().to_string()),
            cursor_chat_id: None,
            activity_status: None,
            hook_token: Some(hook_token),
            sandbox: None,
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: Some(codebase_instance_id.to_string()),
            codebase_session_id: Some(codebase_session_id.to_string()),
        };
        tddy_core::write_session_metadata(&session_dir, &meta)
            .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;

        log::info!(
            target: "tddy_daemon::connection_service",
            "started split claude-cli session {session_id} pid={} codebase_daemon={codebase_instance_id} codebase_session={codebase_session_id}",
            handle.pid
        );

        Ok(Response::new(StartSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
            branch_conflict: None,
        }))
    }

    /// How long to wait for the codebase daemon's answer to a split session's forwarded start.
    ///
    /// Not the ordinary [`PEER_FORWARD_TIMEOUT`]: the peer serves this call by resolving the project
    /// — cloning it if it does not have it yet — and cutting a worktree, work it bounds by its own
    /// `spawn_worker_request_timeout` (5 minutes by default). Giving up after 30 s would mean
    /// erroring while the peer is still building, which is the state that used to strand a worktree.
    /// This daemon can only assume the peer's budget matches its own, so it waits that budget out
    /// plus one ordinary forward deadline of round-trip headroom. A peer configured with a *larger*
    /// budget still times out here — the teardown at the call site is what keeps that from becoming
    /// an orphan.
    ///
    /// The cost is that a peer whose RPC participant is gone surfaces after this wait rather than
    /// after 30 s. Accepted: the placement check already required the peer to be visible in the
    /// common room moments earlier, so that is the rarer failure, and the alternative trades a rare
    /// slow error for a routine orphaned worktree.
    pub fn split_forward_deadline(&self) -> Duration {
        self.config.spawn_worker_request_timeout()
            + crate::livekit_peer_discovery::PEER_FORWARD_TIMEOUT
    }

    /// Delete the `workspace` session holding a split session's worktree on `codebase_instance_id`.
    ///
    /// Used only to unwind a failed start, where the caller already has an error to return: the
    /// failure that got us here is the more useful one, so a teardown failure is logged with the
    /// orphaned session named rather than replacing it.
    ///
    /// `unwinding` is what the start failed with, which is the only thing that decides how much the
    /// peer's answer proves — see the `peer_has_no_such_session` arm below.
    async fn tear_down_codebase_session(
        &self,
        slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
        unwinding: SplitStartFailure,
    ) {
        let request = DeleteSessionRequest {
            session_token: session_token.to_string(),
            session_id: codebase_session_id.to_string(),
        };
        match crate::livekit_peer_discovery::forward_delete_session_via_livekit(
            slot,
            codebase_instance_id,
            &request,
        )
        .await
        {
            Ok(_) => log::info!(
                "StartSession: tore down workspace session {codebase_session_id} on daemon {codebase_instance_id} after a failed split start"
            ),
            // A start that failed before the peer created anything is the ordinary case here — the
            // teardown is issued blind, because a forward that never answered leaves this side
            // unable to tell what the peer got as far as building.
            //
            // "Not there" is a statement about *now*, not about the whole start. After a forward
            // deadline the peer may still be cutting the worktree, so it can answer this honestly
            // and create the session moments later — the one case that still orphans a checkout,
            // and the reason that case is a warning an operator can grep for rather than an info
            // line saying nothing was created.
            Err(e) if peer_has_no_such_session(&e) => match unwinding {
                SplitStartFailure::PeerAnswered => log::info!(
                    "StartSession: daemon {codebase_instance_id} did not have workspace session {codebase_session_id} at teardown time, after a failed split start"
                ),
                SplitStartFailure::ForwardDeadline => log::warn!(
                    "StartSession: daemon {codebase_instance_id} did not have workspace session {codebase_session_id} at teardown time, but the forwarded start had already timed out: if that daemon was still building the worktree it may create the session after this teardown, leaving an orphaned checkout there"
                ),
            },
            Err(e) => log::error!(
                "StartSession: could not delete workspace session {codebase_session_id} on daemon {codebase_instance_id} after a failed split start ({e}); its worktree is now orphaned there"
            ),
        }
    }

    /// Delete the `workspace` session paired with a split session, on the daemon that holds its
    /// worktree. A no-op for a co-located session, which records no pairing.
    ///
    /// Unlike the failed-start teardown, a failure here is returned: `DeleteSession` succeeding
    /// while the worktree survives on another host is exactly the silent leak this pairing exists to
    /// prevent, so the message names the session left behind and where.
    async fn delete_paired_codebase_session(
        &self,
        sessions_base: &Path,
        session_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        let session_dir = unified_session_dir_path(sessions_base, session_id);
        let Ok(meta) = read_session_metadata(&session_dir) else {
            return Ok(());
        };
        let Some((codebase_daemon, codebase_session)) = crate::split_session::split_pairing(&meta)
        else {
            return Ok(());
        };

        let slot = self.common_room_slot("DeleteSession")?;
        // `common_room_slot` only proves this daemon is *configured* for a common room, not that it
        // is currently joined to one — the discovery loop empties this slot on every disconnect.
        // The distinction is load-bearing here: a forward attempted with no room fails locally with
        // `failed_precondition`, which is the same code the peer returns for "I do not have that
        // session". Without this check the two are indistinguishable, and a momentary disconnect
        // would be read as "already torn down", completing the local delete and stranding the
        // worktree on the codebase host — the exact leak the paired teardown exists to prevent.
        if slot.read().await.is_none() {
            return Err(Status::failed_precondition(format!(
                "cannot reach the common room to delete the paired workspace session \
                 {codebase_session} on daemon {codebase_daemon}, so its worktree's fate is unknown; \
                 this session was left in place — retry once the daemons can see each other, or \
                 delete that session on {codebase_daemon} directly and retry"
            )));
        }
        match crate::livekit_peer_discovery::forward_delete_session_via_livekit(
            slot,
            codebase_daemon,
            &DeleteSessionRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session.to_string(),
            },
        )
        .await
        {
            Ok(_) => log::info!(
                "DeleteSession: deleted paired workspace session {codebase_session} on daemon {codebase_daemon}"
            ),
            // The peer answering "I do not have that session" is the state this call exists to
            // reach, not a failure to reach it: an operator may have deleted it there directly, or
            // an earlier attempt may have succeeded on the peer and then failed locally. Continuing
            // is idempotency — the worktree is provably gone with the session that owned it. It is
            // deliberately *not* the treatment for any other outcome: an unreachable or failing peer
            // leaves the worktree's fate unknown, and unknown is refused below.
            Err(e) if peer_has_no_such_session(&e) => log::info!(
                "DeleteSession: daemon {codebase_daemon} no longer has the paired workspace session {codebase_session} ({e}); it was already torn down, so this session's deletion continues"
            ),
            Err(e) => {
                return Err(Status::internal(format!(
                    "could not delete the workspace session {codebase_session} holding this session's worktree on daemon {codebase_daemon} ({e}); its worktree would be orphaned, so the deletion was refused"
                )))
            }
        }
        Ok(())
    }

    /// The one implementation behind both `StartSession` and `StreamStartSession`.
    ///
    /// `progress` is where attachment materialization reports to: the stream's sender for the
    /// streaming entry point, [`AttachmentProgressSink::discarding`] for the unary one. Nothing
    /// else differs between the two, so the unary path stays byte-for-byte what it was.
    async fn start_session_core(
        &self,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        // An agent id names either a coding backend from the config allowlist or an assistant in
        // this daemon's registry — the registry is a def source on equal footing with the YAML
        // (`resolvable_agent_defs`), so an assistant `ListAgents` offers must also be startable.
        let agent_trim = req.agent.trim();
        let agent_def = match agent_trim.is_empty() {
            true => None,
            false => self.agent_def_for_spawn(agent_trim, &github_user).await?,
        };
        if !agent_trim.is_empty() && agent_def.is_none() {
            let allowed = self.config.allowed_agents();
            if !allowed.is_empty() && !allowed.iter().any(|a| a.id == agent_trim) {
                return Err(Status::invalid_argument(format!(
                    "agent id {:?} is not listed in allowed_agents (configure daemon YAML) and is \
                     not an assistant in this daemon's registry",
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

        // The agent runs here; where its worktree goes is the second, independent placement. A
        // refused split is a malformed request, so it is classified before anything is created and
        // before the project is provisioned — a session whose codebase host is wrong should not
        // leave a clone behind on the way to being rejected.
        let placement = classify_codebase_placement(
            &local_id,
            &req.codebase_daemon_instance_id,
            &eligible_ids,
            req.managed_codebase,
            req.session_type.trim(),
        )
        .map_err(|msg| {
            log::info!("StartSession: rejected codebase placement: {msg}");
            Status::invalid_argument(msg)
        })?;

        // Checked here, alongside the other request-shape decisions, so a session type that does not
        // honour a caller-chosen id refuses it before anything is created rather than generating one
        // and leaving the caller pointing at a session that does not exist.
        let caller_chosen_session_id =
            resolve_caller_chosen_session_id(&req.requested_session_id, req.session_type.trim())?;

        // Validate cheap, session-type-specific inputs before the (potentially expensive) project
        // auto-provision below: claude-cli always requires a model, so reject an empty one up front
        // — a bad request should fail fast with INVALID_ARGUMENT, not a project NotFound. The
        // per-session-type handlers re-check as defense-in-depth (and for the resume/child paths).
        if req.session_type.trim() == "claude-cli" && req.model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for claude-cli sessions",
            ));
        }
        if req.session_type.trim() == "cursor-cli" && req.model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for cursor-cli sessions",
            ));
        }

        // A split session has no repository here, so it skips the project auto-provision below and
        // the whole worktree-bearing dispatch: the codebase host resolves the project against its
        // own filesystem.
        if let CodebasePlacement::Split {
            codebase_instance_id,
        } = &placement
        {
            return self
                .start_split_claude_cli_session(os_user, codebase_instance_id, &req, progress)
                .await;
        }

        // Auto-provision the project's working copy on this host before dispatching to any session
        // type: if the project isn't cloned here yet (registered-but-missing, or known only on a
        // peer), clone it into the host's base location so the session can start on a host that
        // doesn't have the project yet. A truly unknown project surfaces as NotFound.
        {
            let project_id = req.project_id.trim();
            if !project_id.is_empty() {
                let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                    .ok_or_else(|| Status::internal("could not resolve projects path"))?;
                self.ensure_project_available_for_start(
                    os_user,
                    &projects_dir,
                    project_id,
                    &req.session_token,
                    req.agent_clone.as_ref(),
                )
                .await?;
            }
        }

        // A base session that cannot seed a stack is refused here, before the session-type dispatch
        // and before anything is created, so the new-session form can show the reason in its error
        // strip rather than navigating away from a session that came up unseeded.
        if !req.pr_stack_base_session_id.trim().is_empty() {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            // The requesting project's repository, which the base session's must be. Resolved here
            // rather than reusing the tool branch's lookup further down, because the whole value of
            // this refusal is that it happens before any of that runs. A seeded orchestrator without
            // a project has no repository to be scoped to, so it is refused rather than exempted.
            let project_id = req.project_id.trim();
            if project_id.is_empty() {
                return Err(Status::invalid_argument(
                    "project_id is required to seed a PR stack from a base session",
                ));
            }
            let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve projects path"))?;
            let project = project_storage::find_project(&projects_dir, project_id)
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("project not found"))?;
            validate_stack_seed_base_session(
                &sessions_base,
                &req.recipe,
                &req.pr_stack_base_session_id,
                Path::new(&project.main_repo_path),
            )
            .map_err(crate::connection_tonic_adapter::to_rpc_status)?;
        }

        // A requested new branch another session already owns is refused here, before the
        // session-type dispatch — so one check covers tool, claude-cli, cursor-cli and workspace, and
        // so nothing has been created yet when it fires.
        if let Some(conflict) = self.owned_branch_conflict(os_user, &req).await? {
            log::info!(
                "StartSession: refusing branch {:?} owned by session {}",
                conflict.branch,
                conflict
                    .owner
                    .as_ref()
                    .map(|o| o.session_id.as_str())
                    .unwrap_or_default()
            );
            return Ok(Response::new(StartSessionResponse {
                branch_conflict: Some(conflict),
                ..Default::default()
            }));
        }

        // --- workspace branch: no LiveKit, no PTY; resolves project, creates a git worktree ---
        if req.session_type.trim() == "workspace" {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            let session_id = match caller_chosen_session_id {
                // Creating a session over an existing one would overwrite its `.session.yaml` and
                // leave that session's worktree with nothing pointing at it, so a taken id is
                // refused rather than reused.
                Some(chosen) => {
                    if unified_session_dir_path(&sessions_base, &chosen).exists() {
                        return Err(Status {
                            code: tddy_rpc::Code::AlreadyExists,
                            message: format!(
                                "requested_session_id {chosen:?} already names a session on this daemon"
                            ),
                        });
                    }
                    chosen
                }
                None => Uuid::now_v7().to_string(),
            };
            self.prepare_session_attachments(&AttachmentMaterialization {
                session_token: &req.session_token,
                os_user,
                sessions_base: &sessions_base,
                session_id: &session_id,
                attachments: &req.attachments,
                progress,
            })
            .await?;
            let timeout = self.config.spawn_worker_request_timeout();
            // The room serves this very service: a participant of a session room reaches the same
            // `ExecuteTool` / `ReadHostDocument` surface a caller reaches over the common room or
            // HTTP, on the same daemon, rooted at the checkout it just made.
            // An agent clone is a workspace session with a job: its checkout is a mirror rather than
            // a branch to work on, so it is cut differently, and it then joins the facilitating
            // daemon's session room and keeps itself equal to that session's worktree. Readiness is
            // reported from there, because only this daemon can say when the mirror has caught up.
            let Some(placement) = req.agent_clone.clone() else {
                return workspace_session::start_workspace_session(
                    os_user,
                    &session_id,
                    sessions_base,
                    req.project_id.trim(),
                    &workspace_session::WorkspaceBranchIntent {
                        branch_worktree_intent: req.branch_worktree_intent.trim(),
                        new_branch_name: req.new_branch_name.trim(),
                        selected_integration_base_ref: req.selected_integration_base_ref.trim(),
                        selected_branch_to_work_on: req.selected_branch_to_work_on.trim(),
                    },
                    &self.tddy_data_dir,
                    timeout,
                )
                .await;
            };
            let started = workspace_session::start_agent_clone_session(
                os_user,
                &session_id,
                sessions_base.clone(),
                req.project_id.trim(),
                &self.tddy_data_dir,
                timeout,
            )
            .await?;
            self.start_hosted_agent_clone(
                &placement,
                &sessions_base,
                &session_id,
                req.project_id.trim(),
                &req.session_token,
            )
            .await?;
            return Ok(started);
        }

        // --- claude-cli branch: no LiveKit; resolves project and creates a real git worktree ---
        if req.session_type.trim() == "claude-cli" {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            let session_id = Uuid::now_v7().to_string();
            self.prepare_session_attachments(&AttachmentMaterialization {
                session_token: &req.session_token,
                os_user,
                sessions_base: &sessions_base,
                session_id: &session_id,
                attachments: &req.attachments,
                progress,
            })
            .await?;
            let stack_parent_for_claude_cli: Option<String> = {
                let t = req.stack_parent.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            };
            // A managed-codebase claude-cli session with a recipe is launched workflow-aware. An
            // unknown recipe is a request error (never silently ignored). Non-managed sessions and
            // managed sessions without a recipe keep the plain launch (managed_recipe = None).
            let managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>> = if req
                .managed_codebase
                && !req.recipe.trim().is_empty()
            {
                Some(
                    tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(req.recipe.trim())
                        .map_err(Status::invalid_argument)?,
                )
            } else {
                None
            };

            if req.sandbox {
                return self
                    .start_sandboxed_claude_cli_session(
                        os_user,
                        &session_id,
                        sessions_base,
                        req.model.trim(),
                        req.project_id.trim(),
                        req.repo_path.trim(),
                        req.branch_worktree_intent.trim(),
                        req.new_branch_name.trim(),
                        req.selected_integration_base_ref.trim(),
                        req.selected_branch_to_work_on.trim(),
                        req.initial_prompt.trim(),
                        &req.claude_args,
                        req.permission_mode.trim(),
                        req.dangerously_skip_permissions,
                        stack_parent_for_claude_cli.as_deref(),
                        req.managed_codebase,
                        &req.specialized_agents,
                        managed_recipe,
                        req.semantic_index,
                        req.create_remote_branch,
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
                    req.dangerously_skip_permissions,
                    stack_parent_for_claude_cli.as_deref(),
                    managed_recipe,
                    req.semantic_index,
                    req.create_remote_branch,
                )
                .await;
        }

        // --- cursor-cli branch: no LiveKit; spawns Cursor Agent CLI in a PTY worktree ---
        if req.session_type.trim() == "cursor-cli" {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            let session_id = Uuid::now_v7().to_string();
            self.prepare_session_attachments(&AttachmentMaterialization {
                session_token: &req.session_token,
                os_user,
                sessions_base: &sessions_base,
                session_id: &session_id,
                attachments: &req.attachments,
                progress,
            })
            .await?;
            let managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>> = if req
                .managed_codebase
                && !req.recipe.trim().is_empty()
            {
                Some(
                    tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(req.recipe.trim())
                        .map_err(Status::invalid_argument)?,
                )
            } else {
                None
            };
            if req.sandbox {
                return self
                    .start_sandboxed_cursor_cli_session(
                        os_user,
                        &session_id,
                        sessions_base,
                        req.model.trim(),
                        req.project_id.trim(),
                        req.branch_worktree_intent.trim(),
                        req.new_branch_name.trim(),
                        req.selected_integration_base_ref.trim(),
                        req.selected_branch_to_work_on.trim(),
                        Some(req.stack_parent.trim()).filter(|s| !s.is_empty()),
                        req.initial_prompt.trim(),
                        req.managed_codebase,
                        &req.specialized_agents,
                        managed_recipe,
                        req.semantic_index,
                        req.create_remote_branch,
                    )
                    .await;
            }
            // Resolved before the spawn, not after: an agent the request names and this daemon
            // cannot resolve fails the start, exactly as it does on the sandboxed paths, rather
            // than persisting a roster entry that resolves to nothing on the next resume.
            let started_agents = started_roster(
                &self
                    .resolve_specialized_agent_defs(&req.specialized_agents)
                    .await?,
                &local_instance_id_for_config(&self.config),
            )?;
            return crate::cursor_cli_spawn::spawn_cursor_cli_session_inner(
                &self.config,
                &self.tddy_data_dir,
                &self.claude_cli_manager,
                os_user,
                &session_id,
                sessions_base,
                req.model.trim(),
                req.project_id.trim(),
                req.branch_worktree_intent.trim(),
                req.new_branch_name.trim(),
                req.selected_integration_base_ref.trim(),
                req.selected_branch_to_work_on.trim(),
                req.repo_path.trim(),
                Some(req.stack_parent.trim()).filter(|s| !s.is_empty()),
                req.initial_prompt.trim(),
                req.managed_codebase,
                &started_agents,
                managed_recipe,
                req.semantic_index,
                req.create_remote_branch,
                &self.task_registry,
                &self.session_room_host(),
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
        // A spawned `tddy-coder` resolves `--agent` against the builtins and `<tddyhome>/agents`
        // only; this daemon's registry is a source it cannot read. So the def this daemon already
        // resolved travels with the spawn as `--agent-def`, and the child creates its backend from
        // that rather than falling through to a different agent entirely.
        let agent_def_for_spawn: Option<String> = agent_def
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| Status::internal(format!("failed to serialize agent def: {e}")))?;
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
        // Already validated above; the orchestrator's own process is what seeds the stack, because
        // the session that owns a `changeset.yaml` is the process that writes it.
        let stack_seed_base_session_for_spawn: Option<String> = {
            let t = req.pr_stack_base_session_id.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let model_for_spawn: Option<String> = {
            let t = req.model.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let timeout = self.config.spawn_worker_request_timeout();
        let daemon_log = self.config.log.clone();
        let startup_watch = spawner::StartupWatch::from_config(&self.config);
        let coder_config_path = self.config.coder_config_path.clone();
        // Grill-me tool sessions relay `spawn_conversation` back over a per-session unix socket.
        // Because the coder needs the socket path (and orchestrator id) at spawn time — and the
        // socket path is what crosses the forked `spawn_worker` boundary — bind it and pre-generate
        // the session id BEFORE the spawn, so both the worker and direct paths carry it identically.
        let enable_conversation_spawn = recipe_for_spawn
            .as_deref()
            .map(recipe_enables_conversation_spawn)
            .unwrap_or(false);
        let (mut pre_session_id, host_session_socket): (Option<String>, Option<String>) =
            if enable_conversation_spawn {
                let sid = Uuid::now_v7().to_string();
                let sock = self
                    .spawn_host_session_socket(
                        &sid,
                        &os_user,
                        &pid_for_spawn,
                        model_for_spawn.clone(),
                    )
                    .await;
                (Some(sid), sock)
            } else {
                (None, None)
            };
        let tool_session_id = pre_session_id
            .clone()
            .unwrap_or_else(|| Uuid::now_v7().to_string());
        if enable_conversation_spawn || !req.attachments.is_empty() {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                &os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            self.prepare_session_attachments(&AttachmentMaterialization {
                session_token: &req.session_token,
                os_user: &os_user,
                sessions_base: &sessions_base,
                session_id: &tool_session_id,
                attachments: &req.attachments,
                progress,
            })
            .await?;
            pre_session_id = Some(tool_session_id);
        }
        let result = match crate::supervisor_client::spawn_backend_choice(&self.config) {
            crate::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                let coder_log_yaml = spawner::coder_log_config_yaml(coder_config_path.as_deref());
                let spawn_req = spawn_worker::build_spawn_request(
                    &os_user,
                    &tool_path,
                    &tddy_data_dir_for_spawn,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: None,
                        new_session_id: pre_session_id.as_deref(),
                        project_id: Some(pid_for_spawn.as_str()),
                        agent: agent_for_spawn.as_deref(),
                        agent_def_json: agent_def_for_spawn.as_deref(),
                        mouse: spawn_mouse,
                        recipe: recipe_for_spawn.as_deref(),
                        stack_parent: stack_parent_for_spawn.as_deref(),
                        stack_seed_base_session: stack_seed_base_session_for_spawn.as_deref(),
                        model: model_for_spawn.as_deref(),
                        host_session_socket: host_session_socket.as_deref(),
                    },
                    daemon_log.as_ref(),
                    coder_log_yaml,
                    startup_watch,
                );
                await_supervised_with_timeout(
                    timeout,
                    "StartSession: spawn via tddy-supervisor",
                    crate::supervisor_spawn::spawn_session_via_supervisor(&socket_path, &spawn_req),
                )
                .await?
            }
            crate::supervisor_client::SpawnBackendChoice::ForkedWorker => {
                spawn_blocking_with_timeout(timeout, "StartSession: spawn", move || {
                    log::debug!(
                        "StartSession: spawn_blocking running, using_spawn_worker={}",
                        spawn_client.is_some()
                    );
                    let pid = Some(pid_for_spawn.as_str());
                    let agent = agent_for_spawn.as_deref();
                    let agent_def = agent_def_for_spawn.as_deref();
                    let recipe = recipe_for_spawn.as_deref();
                    let stack_parent = stack_parent_for_spawn.as_deref();
                    let stack_seed_base_session = stack_seed_base_session_for_spawn.as_deref();
                    let model = model_for_spawn.as_deref();
                    let new_session_id = pre_session_id.as_deref();
                    let host_socket = host_session_socket.as_deref();
                    let coder_log_yaml =
                        spawner::coder_log_config_yaml(coder_config_path.as_deref());
                    if let Some(ref client) = spawn_client {
                        let spawn_req = spawn_worker::build_spawn_request(
                            &os_user,
                            &tool_path,
                            &tddy_data_dir_for_spawn,
                            &repo_path,
                            &livekit,
                            SpawnOptions {
                                resume_session_id: None,
                                new_session_id,
                                project_id: pid,
                                agent,
                                agent_def_json: agent_def,
                                mouse: spawn_mouse,
                                recipe,
                                stack_parent,
                                stack_seed_base_session,
                                model,
                                host_session_socket: host_socket,
                            },
                            daemon_log.as_ref(),
                            coder_log_yaml,
                            startup_watch,
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
                                new_session_id,
                                project_id: pid,
                                agent,
                                agent_def_json: agent_def,
                                mouse: spawn_mouse,
                                recipe,
                                stack_parent,
                                stack_seed_base_session,
                                model,
                                host_session_socket: host_socket,
                            },
                            child_log_level.as_str(),
                            child_log_format.as_str(),
                            coder_log_yaml.as_deref(),
                            startup_watch,
                        )
                    }
                })
                .await?
            }
        };
        log::debug!(
            "StartSession: spawn returned, session_id={}",
            result.session_id
        );
        self.maybe_spawn_telegram_observer(&result.session_id, result.grpc_port);
        Ok(Response::new(StartSessionResponse {
            session_id: result.session_id,
            livekit_room: result.livekit_room,
            livekit_url: result.livekit_url,
            livekit_server_identity: result.livekit_server_identity,
            branch_conflict: None,
        }))
    }
}

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
/// `CancelAgentConversation`; this handler decodes each, calls the matching typed method on the
/// `Arc<ConnectionServiceImpl>` it holds, and returns the encoded response — unary for the two
/// unary RPCs, a server stream of encoded frames for the two streaming ones. `tonic::Status`
/// errors are carried back to the in-jail caller as a single terminal `RpcStreamFrame` with
/// `error` set, which the runner's relay turns into the `tddy_rpc::Status` the caller sees.
struct DaemonRpcHandler {
    conn: Arc<ConnectionServiceImpl>,
}

#[async_trait::async_trait]
impl tddy_sandbox_runner::HostRpcHandler for DaemonRpcHandler {
    async fn handle_rpc(&self, service: &str, method: &str, payload: &[u8]) -> tddy_rpc::RpcResult {
        use prost::Message;
        use tddy_rpc::Request;
        // Only the four RPCs the runner forwards ride this bridge; anything else is a wiring bug
        // (the runner's `ToolExecService` would not forward it) and is refused with `not_found`
        // rather than reaching arbitrary `ConnectionService` surface from inside a jail.
        match (service, method) {
            ("connection.ConnectionService", "StreamSessionAgents") => {
                let req = match StreamSessionAgentsRequest::decode(payload) {
                    Ok(r) => r,
                    Err(e) => {
                        return tddy_rpc::RpcResult::Unary(Err(
                            tddy_rpc::Status::invalid_argument(format!(
                                "decode StreamSessionAgentsRequest: {e}"
                            )),
                        ));
                    }
                };
                match self.conn.stream_session_agents(Request::new(req)).await {
                    Ok(resp) => {
                        let mut stream = resp.into_inner();
                        let (tx, rx) = tokio::sync::mpsc::channel(16);
                        tokio::spawn(async move {
                            while let Some(frame) = stream.next().await {
                                let encoded = frame.map(|roster| roster.encode_to_vec());
                                if tx.send(encoded).await.is_err() {
                                    return;
                                }
                            }
                            // The daemon's stream ended cleanly; the receiver observes
                            // end-of-stream when this sender drops.
                        });
                        tddy_rpc::RpcResult::ServerStream(Ok(rx))
                    }
                    Err(status) => tddy_rpc::RpcResult::ServerStream(Err(status)),
                }
            }
            ("connection.ConnectionService", "OpenAgentConversation") => {
                let req = match OpenAgentConversationRequest::decode(payload) {
                    Ok(r) => r,
                    Err(e) => {
                        return tddy_rpc::RpcResult::Unary(Err(
                            tddy_rpc::Status::invalid_argument(format!(
                                "decode OpenAgentConversationRequest: {e}"
                            )),
                        ));
                    }
                };
                match self.conn.open_agent_conversation(Request::new(req)).await {
                    Ok(resp) => tddy_rpc::RpcResult::Unary(Ok(resp.into_inner().encode_to_vec())),
                    Err(status) => tddy_rpc::RpcResult::Unary(Err(status)),
                }
            }
            ("connection.ConnectionService", "PromptAgentConversation") => {
                let req = match PromptAgentConversationRequest::decode(payload) {
                    Ok(r) => r,
                    Err(e) => {
                        return tddy_rpc::RpcResult::Unary(Err(
                            tddy_rpc::Status::invalid_argument(format!(
                                "decode PromptAgentConversationRequest: {e}"
                            )),
                        ));
                    }
                };
                match self.conn.prompt_agent_conversation(Request::new(req)).await {
                    Ok(resp) => {
                        let mut stream = resp.into_inner();
                        let (tx, rx) = tokio::sync::mpsc::channel(16);
                        tokio::spawn(async move {
                            while let Some(frame) = stream.next().await {
                                let encoded = frame.map(|chunk| chunk.encode_to_vec());
                                if tx.send(encoded).await.is_err() {
                                    return;
                                }
                            }
                        });
                        tddy_rpc::RpcResult::ServerStream(Ok(rx))
                    }
                    Err(status) => tddy_rpc::RpcResult::ServerStream(Err(status)),
                }
            }
            ("connection.ConnectionService", "CancelAgentConversation") => {
                let req = match CancelAgentConversationRequest::decode(payload) {
                    Ok(r) => r,
                    Err(e) => {
                        return tddy_rpc::RpcResult::Unary(Err(
                            tddy_rpc::Status::invalid_argument(format!(
                                "decode CancelAgentConversationRequest: {e}"
                            )),
                        ));
                    }
                };
                match self.conn.cancel_agent_conversation(Request::new(req)).await {
                    Ok(resp) => tddy_rpc::RpcResult::Unary(Ok(resp.into_inner().encode_to_vec())),
                    Err(status) => tddy_rpc::RpcResult::Unary(Err(status)),
                }
            }
            _ => tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::not_found(format!(
                "DaemonRpcHandler does not serve {service}/{method}"
            )))),
        }
    }
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
        // A registry this daemon has but cannot read is an error, not "there are no assistants" —
        // a session started against a missing agent id fails much later and much less clearly.
        let assistants = match &self.model_registry {
            Some(registry) => registry
                .list_assistants()
                .await
                .map_err(tddy_rpc::Status::from)?,
            None => Vec::new(),
        };
        let agents: Vec<AgentInfo> = agent_allowlist_rows(&self.config, &assistants)
            .into_iter()
            .map(|row| AgentInfo {
                id: row.id,
                label: row.display_label,
            })
            .collect();
        log::info!("list_agents RPC: returning {} agent(s)", agents.len());
        Ok(Response::new(ListAgentsResponse { agents }))
    }

    /// Enumerate the models an agent supports by shelling out to `tddy-tools list-models` as the
    /// caller's OS user. Results are cached per (agent, daemon) for a short TTL. A failed probe is
    /// surfaced as an RPC error — never masked with a fallback catalog.
    ///
    /// Runs the probe on the local daemon; `daemon_instance_id` participates only in the cache key
    /// (cross-daemon forwarding is not wired here — the web fetches models from the daemon it is
    /// already connected to).
    async fn list_agent_models(
        &self,
        request: Request<ListAgentModelsRequest>,
    ) -> Result<Response<ListAgentModelsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?
            .to_string();

        let agent = req.agent.trim().to_string();
        if agent.is_empty() {
            return Err(Status::invalid_argument("agent is required"));
        }

        // Key by OS user: cursor / ACP catalogs (and the "current, default" model) are
        // account-specific, so one user's list must never be served to another from the cache.
        let cache_key = format!(
            "{}\u{1f}{}\u{1f}{}",
            os_user,
            req.daemon_instance_id.trim(),
            agent
        );
        if let Ok(cache) = agent_models_cache().lock() {
            if let Some((cached_at, resp)) = cache.get(&cache_key) {
                if cached_at.elapsed() < AGENT_MODELS_CACHE_TTL {
                    return Ok(Response::new(resp.clone()));
                }
            }
        }

        let tools_path = self.resolve_tddy_tools_path();
        // Cursor's model probe must hand tddy-tools the resolved absolute `agent` path (as the PTY
        // spawn does), so the impersonated child execs a fully-qualified binary instead of doing a
        // PATH lookup that lacks the install dir. Only forward an absolute path — a bare-name
        // resolution keeps the existing behavior (no `--cursor-cli-path`).
        let cursor_cli_path = (agent == "cursor")
            .then(|| crate::config::resolve_cursor_binary_path(&self.config))
            .filter(|p| std::path::Path::new(p).is_absolute())
            .map(std::path::PathBuf::from);
        let probe_args = list_models_probe_args(&agent, cursor_cli_path.as_deref());
        let probe = tokio::task::spawn_blocking(move || {
            spawner::run_capture_as_user(&os_user, &tools_path, &probe_args)
        })
        .await
        .map_err(|e| Status::internal(format!("model probe join error: {e}")))?
        .map_err(|e| Status::failed_precondition(format!("model probe failed: {e}")))?;

        let resp = parse_agent_models_json(&probe)?;

        if let Ok(mut cache) = agent_models_cache().lock() {
            cache.insert(cache_key, (std::time::Instant::now(), resp.clone()));
        }
        Ok(Response::new(resp))
    }

    /// Resolved specialized-agent defs available to wire into a managed-codebase session — every
    /// source a name can resolve against here, so `<tddyhome>/agents/*.yaml` (see
    /// docs/ft/coder/specialized-subagents.md) *and* this daemon's registry assistants.
    ///
    /// Answered from [`Self::resolvable_agent_defs`], which is also what an attach resolves the id
    /// it is handed against: what a picker is offered and what it can then attach are one list, not
    /// two that can drift. Advertising less than that is what made an assistant created in Models &
    /// Agents invisible to the roster while being perfectly attachable by name.
    ///
    /// Every row is stamped with this daemon's instance id and the qualified `agent_id` it is
    /// attached by. A picker fans this call out across every common-room daemon, and two of them
    /// routinely answer with a def called `explorer`: without the stamp the merged list cannot say
    /// which host offers which row, and the id the picker sends would be a guess rather than the
    /// one the serving daemon minted.
    ///
    /// A def whose own name contains `@` is dropped with a warning: its qualified id would parse
    /// back as a different pair, so advertising it would hand a picker an id that routes elsewhere.
    async fn list_subagents(
        &self,
        _request: Request<ListSubagentsRequest>,
    ) -> Result<Response<ListSubagentsResponse>, Status> {
        log::debug!("list_subagents RPC: resolving agent defs");
        let daemon_instance_id = local_instance_id_for_config(&self.config);
        let defs = self.resolvable_agent_defs().await?;
        let resolved = defs.len();
        let subagents: Vec<SubagentInfo> = defs
            .into_iter()
            .filter_map(|def| match subagent_info(&def, &daemon_instance_id) {
                Ok(info) => Some(info),
                Err(e) => {
                    log::warn!("list_subagents RPC: not advertising a def — {e}");
                    None
                }
            })
            .collect();
        // An empty answer has three very different causes — this daemon has no defs, its registry
        // was never wired in, or a def was dropped on the way out — and "returning 0" told them
        // apart in none of them. Naming the sources is what makes an empty picker diagnosable from
        // the log alone, on a host whose filesystem is not to hand.
        log::info!(
            "list_subagents RPC: returning {} subagent(s) of {} resolved def(s) [agents dir {}, \
             model registry {}]: {:?}",
            subagents.len(),
            resolved,
            self.tddy_data_dir.join("agents").display(),
            if self.model_registry.is_some() {
                "attached"
            } else {
                "absent"
            },
            subagents.iter().map(|s| &s.agent_id).collect::<Vec<_>>()
        );
        Ok(Response::new(ListSubagentsResponse { subagents }))
    }

    // ── Session agent roster (docs/ft/daemon/session-agent-roster.md) ─────────────────────────

    /// Attach one agent to a live session, or report the roster unchanged when it is already there.
    ///
    /// The order is the contract: the caller is authenticated, then the id is resolved, then the
    /// session is checked for being able to enforce what the agent withdraws, then the roster is
    /// written. Every step before the write is one that must not happen for a caller who turns out
    /// not to be allowed — resolving a remote id contacts a peer and provisions a checkout on it
    /// (PRD AC12).
    async fn attach_session_agent(
        &self,
        request: Request<AttachSessionAgentRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        let mut record = self.roster_record_for_agent_id(&req.agent_id).await?;
        refuse_unenforceable_withdrawal(&req.session_id, &session_dir, &record)?;
        // An agent owned by a peer reads a checkout on that peer, so the entry has to name one
        // before it is written. Claiming it is also what opens the session's room — and both happen
        // before the roster is touched, so an attach that cannot be completed leaves the session
        // looking exactly as it did (PRD § What attach does: "no roster entry, no half-built clone,
        // no room membership").
        let mut claimed = None;
        if record.daemon_instance_id != local_instance_id_for_config(&self.config) {
            let clone = self
                .claim_agent_clone(
                    &req.session_id,
                    &session_dir,
                    &record.daemon_instance_id,
                    &req.session_token,
                )
                .await?;
            record.codebase_session_id = Some(clone.codebase_session_id.clone());
            claimed = Some((record.daemon_instance_id.clone(), clone));
        }
        // A roster this daemon could not write is an attach that did not happen, and the clone
        // claimed a moment ago is the half of it the peer has already been told to build: without
        // this the caller would be handed an error while a checkout it can no longer name kept being
        // cut on another host ("no roster entry, no half-built clone, no room membership").
        let roster = match self
            .session_agent_rosters
            .attach(&req.session_id, &session_dir, record)
        {
            Ok(roster) => roster,
            Err(e) => {
                if let Some((daemon_instance_id, clone)) = claimed {
                    self.unwind_agent_clone_claim(
                        &req.session_id,
                        &daemon_instance_id,
                        &clone,
                        &req.session_token,
                    )
                    .await;
                }
                return Err(e);
            }
        };
        self.broadcast_roster(&req.session_id, &roster).await;
        log::info!(
            "AttachSessionAgent: session {} holds {} agent(s) at rev {}",
            req.session_id,
            roster.agents.len(),
            roster.rev
        );
        Ok(Response::new(roster))
    }

    /// Detach one agent. An id the roster does not hold is `NOT_FOUND`, never a silent success.
    ///
    /// The entry is removed first and the checkout torn down after, in that order: a teardown that
    /// ran first and then failed to remove the entry would leave the roster naming a checkout that
    /// is gone, which is the state a prompt is served from.
    async fn detach_session_agent(
        &self,
        request: Request<DetachSessionAgentRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        let detached =
            self.session_agent_rosters
                .entry(&req.session_id, &session_dir, &req.agent_id)?;
        let roster =
            self.session_agent_rosters
                .detach(&req.session_id, &session_dir, &req.agent_id)?;
        self.cancel_conversations_with(&req.session_token, &req.session_id, &req.agent_id)
            .await;

        // The clone survives while another agent on that host still reads it — two agents on one
        // host share one checkout, so the last one out is what removes it.
        if let Some(record) = detached.filter(|r| r.codebase_session_id.is_some()) {
            let still_used = !self
                .session_agent_rosters
                .agents_owned_by(&req.session_id, &session_dir, &record.daemon_instance_id)?
                .is_empty();
            if !still_used {
                let codebase_session_id = record
                    .codebase_session_id
                    .clone()
                    .expect("filtered to entries naming a clone");
                // The entry is already gone and persisted by now, so the refusal says so: a message
                // that read as "the agent was left attached, retry" would send an operator into a
                // retry that answers NOT_FOUND while the checkout stays exactly where it is.
                if let Err(e) = self
                    .tear_down_agent_clone(
                        &req.session_id,
                        &record.daemon_instance_id,
                        &codebase_session_id,
                        &req.session_token,
                    )
                    .await
                {
                    self.broadcast_roster(&req.session_id, &roster).await;
                    return Err(Status {
                        code: e.code(),
                        message: format!(
                            "agent '{}' was detached from session '{}' (rev {}), but its clone \
                             could not be removed: {}. Retrying the detach reports NOT_FOUND — the \
                             checkout has to be deleted where it is.",
                            req.agent_id,
                            req.session_id,
                            roster.rev,
                            e.message()
                        ),
                    });
                }
            }
        }

        self.broadcast_roster(&req.session_id, &roster).await;
        log::info!(
            "DetachSessionAgent: session {} holds {} agent(s) at rev {}",
            req.session_id,
            roster.agents.len(),
            roster.rev
        );
        Ok(Response::new(roster))
    }

    async fn list_session_agents(
        &self,
        request: Request<ListSessionAgentsRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        Ok(Response::new(
            self.session_agent_rosters
                .snapshot(&req.session_id, &session_dir)?,
        ))
    }

    type StreamSessionAgentsStream = MpscResultStream<SessionAgentRoster>;

    /// The roster, now and on every change.
    ///
    /// The first frame is the current snapshot, taken with the subscription under one lock, so a
    /// late subscriber — the in-jail `tddy-tools` reconnecting, a browser tab opening — needs no
    /// separate priming read and cannot miss a change published between the two.
    async fn stream_session_agents(
        &self,
        request: Request<StreamSessionAgentsRequest>,
    ) -> Result<Response<Self::StreamSessionAgentsStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        let (snapshot, mut published) = self
            .session_agent_rosters
            .subscribe(&req.session_id, &session_dir)?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        if tx.send(Ok(snapshot)).is_err() {
            return Err(Status::internal(
                "StreamSessionAgents: the subscriber went away before its first frame",
            ));
        }
        let session_id = req.session_id.clone();
        tokio::spawn(async move {
            loop {
                match published.recv().await {
                    Ok(roster) => {
                        if tx.send(Ok(roster)).is_err() {
                            break;
                        }
                    }
                    // Every frame is a whole roster, so a subscriber that fell behind is brought
                    // fully current by the next one — the dropped frames carried nothing the
                    // survivor does not also carry.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        log::debug!(
                            "StreamSessionAgents: subscriber to session {session_id} fell {missed} \
                             snapshot(s) behind; the next one supersedes them"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        Ok(Response::new(MpscResultStream { rx }))
    }

    /// Open a conversation with one roster agent.
    ///
    /// A local entry gets a turn loop in this process; a remote entry gets a routing record and the
    /// same call forwarded to its owning daemon. The caller cannot tell which happened, which is the
    /// property that makes remote agents usable at all (PRD AC28).
    ///
    /// A clone that is still being built refuses the open naming its state. Queuing it would make a
    /// 90-second `git clone` look like a hung agent, and serving it would read an empty checkout and
    /// report "not found" for a file that is simply not there yet (AC33).
    async fn open_agent_conversation(
        &self,
        request: Request<OpenAgentConversationRequest>,
    ) -> Result<Response<OpenAgentConversationResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        // Caller-chosen where offered, so an open that times out still leaves the caller able to
        // name — and therefore cancel — whatever this daemon built.
        let conversation_id = match req.conversation_id.trim().is_empty() {
            true => Uuid::now_v7().to_string(),
            false => req.conversation_id.trim().to_string(),
        };

        // This daemon *owns* the agent: the session is another daemon's, its roster is over there,
        // and the def is here. Resolving it against a roster this daemon does not hold would report
        // a session that legitimately is not here as the reason an agent it does own cannot answer.
        let conversation = match self.hosted_clone_for(&req.session_id) {
            Some(clone) => AgentConversation::Local {
                session_id: req.session_id.clone(),
                agent_id: req.agent_id.clone(),
                session: Arc::new(tokio::sync::Mutex::new(
                    self.open_owned_agent_session(&req.agent_id, &clone).await?,
                )),
                closed: Arc::new(tokio::sync::Notify::new()),
            },
            None => {
                let record = self
                    .session_agent_rosters
                    .entry(&req.session_id, &session_dir, &req.agent_id)?
                    .ok_or_else(|| {
                        Status::invalid_argument(format!(
                            "agent '{}' is not attached to session '{}'",
                            req.agent_id, req.session_id
                        ))
                    })?;
                let local_instance_id = local_instance_id_for_config(&self.config);
                match record.daemon_instance_id == local_instance_id {
                    true => AgentConversation::Local {
                        session_id: req.session_id.clone(),
                        agent_id: record.agent_id.clone(),
                        session: Arc::new(tokio::sync::Mutex::new(
                            self.open_local_agent_session(
                                &req.session_id,
                                &record,
                                &req.session_token,
                            )
                            .await?,
                        )),
                        closed: Arc::new(tokio::sync::Notify::new()),
                    },
                    false => {
                        self.refuse_unready_clone(&req.session_id, &record)?;
                        self.refuse_departed_daemon(&record.daemon_instance_id)
                            .await?;
                        self.forward_open_agent_conversation(&req, &record, &conversation_id)
                            .await?;
                        AgentConversation::Remote {
                            session_id: req.session_id.clone(),
                            agent_id: record.agent_id.clone(),
                            daemon_instance_id: record.daemon_instance_id.clone(),
                        }
                    }
                }
            }
        };
        self.agent_conversations
            .lock()
            .await
            .insert(conversation_id.clone(), conversation);
        Ok(Response::new(OpenAgentConversationResponse {
            conversation_id,
        }))
    }

    type PromptAgentConversationStream = MpscResultStream<AgentConversationChunk>;

    /// Prompt an open conversation, streaming the agent's answer back.
    ///
    /// Both variants end with exactly one `last` frame carrying the stop reason, so a consumer never
    /// has to distinguish "said nothing" from "nothing arrived", and a stream that ends without one
    /// was truncated rather than completed.
    async fn prompt_agent_conversation(
        &self,
        request: Request<PromptAgentConversationRequest>,
    ) -> Result<Response<Self::PromptAgentConversationStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        self.roster_session_dir(&req.session_token, &req.session_id)?;

        // Everything the turn needs is taken out of the map here, under one lock, and the guard is
        // dropped before anything is awaited on it.
        let routing = {
            let open = self.agent_conversations.lock().await;
            match open.get(&req.conversation_id) {
                None => {
                    return Err(Status::not_found(format!(
                        "conversation '{}' is not open on session '{}'",
                        req.conversation_id, req.session_id
                    )))
                }
                Some(AgentConversation::Local {
                    session, closed, ..
                }) => PromptRouting::Local {
                    session: Arc::clone(session),
                    closed: Arc::clone(closed),
                },
                Some(AgentConversation::Remote {
                    daemon_instance_id, ..
                }) => PromptRouting::Remote(daemon_instance_id.clone()),
            }
        };

        let (session, closed) = match routing {
            PromptRouting::Local { session, closed } => (session, closed),
            PromptRouting::Remote(daemon_instance_id) => {
                let slot = self.common_room_slot("PromptAgentConversation")?;
                self.refuse_departed_daemon(&daemon_instance_id).await?;
                let rx = crate::livekit_peer_discovery::forward_server_stream_to_peer(
                    slot,
                    &daemon_instance_id,
                    "connection.ConnectionService",
                    "PromptAgentConversation",
                    req.encode_to_vec(),
                    |bytes| {
                        AgentConversationChunk::decode(bytes.as_slice()).map_err(|e| {
                            Status::internal(format!(
                                "decode AgentConversationChunk from peer: {e}"
                            ))
                        })
                    },
                )
                .await?;
                return Ok(Response::new(MpscResultStream { rx }));
            }
        };

        // The turn loop runs here. Spawned rather than awaited so the stream's frames are produced
        // while the caller reads them, and awaited on the *conversation's* lock alone: two prompts on
        // one conversation are still serialized, but the map of open conversations is not held, so a
        // cancel can land while this turn is in flight — which is the only moment a cancel matters.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let conversation_id = req.conversation_id.clone();
        let prompt = req.prompt.clone();
        tokio::spawn(async move {
            let outcome = tokio::select! {
                // Biased so a conversation already closed is reported as closed rather than racing
                // one more turn out of a model.
                biased;
                _ = closed.notified() => {
                    let _ = tx.send(Err(Status::failed_precondition(format!(
                        "conversation '{conversation_id}' was closed while its turn was in flight"
                    ))));
                    return;
                }
                outcome = async { session.lock().await.prompt(&prompt).await } => outcome,
            };
            match outcome {
                // Framed rather than sent whole: over LiveKit anything past MAX_CHUNK_FRAME_BYTES is
                // chunk-framed, and one lost chunk frame wedges the call with no error at all.
                Ok(outcome) => {
                    let content = outcome
                        .content
                        .iter()
                        .map(|block| block.text.as_str())
                        .collect::<Vec<_>>()
                        .join("");
                    for frame in
                        agent_conversation_frames(&content, agent_stop_reason(outcome.stop_reason))
                    {
                        if tx.send(Ok(frame)).is_err() {
                            return;
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(Status::internal(format!(
                        "agent conversation '{conversation_id}' failed: {e}"
                    ))));
                }
            }
        });
        Ok(Response::new(MpscResultStream { rx }))
    }

    /// Cancel an open conversation. An id nothing holds is `NOT_FOUND`, never a silent success — a
    /// caller told a turn was cancelled when it is still running would go on to read a stale answer.
    async fn cancel_agent_conversation(
        &self,
        request: Request<CancelAgentConversationRequest>,
    ) -> Result<Response<CancelAgentConversationResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        self.roster_session_dir(&req.session_token, &req.session_id)?;
        let removed = self
            .agent_conversations
            .lock()
            .await
            .remove(&req.conversation_id);
        match removed {
            None => Err(Status::not_found(format!(
                "conversation '{}' is not open on session '{}'",
                req.conversation_id, req.session_id
            ))),
            Some(AgentConversation::Local { closed, .. }) => {
                // A turn already in flight is interrupted rather than left to finish: the caller has
                // been told the conversation is cancelled, and an answer arriving afterwards would
                // be one it has no reason to expect.
                closed.notify_one();
                Ok(Response::new(CancelAgentConversationResponse {}))
            }
            Some(AgentConversation::Remote {
                daemon_instance_id, ..
            }) => {
                let slot = self.common_room_slot("CancelAgentConversation")?;
                crate::livekit_peer_discovery::forward_to_peer(
                    slot,
                    &daemon_instance_id,
                    "connection.ConnectionService",
                    "CancelAgentConversation",
                    req.encode_to_vec(),
                )
                .await?;
                Ok(Response::new(CancelAgentConversationResponse {}))
            }
        }
    }

    /// The owning daemon telling this one how its clone is doing.
    ///
    /// Pushed rather than polled because only the daemon holding the checkout can say any of it, and
    /// accepted only for a clone this daemon actually asked that daemon for — the report is what
    /// authorizes an entry to start serving prompts.
    ///
    /// Authenticated first, and that is not ceremony: the (session, daemon, clone) triple the store
    /// matches on is published in the session's `session.agents` broadcast, so on the triple alone
    /// any participant that saw a roster frame could report a still-provisioning clone READY and
    /// have the next prompt served from an empty checkout.
    ///
    /// TODO(session-agent-roster): also bind the report to the *reporting participant*. The verified
    /// LiveKit participant identity is known at the transport but is not carried into
    /// `RequestMetadata` — `sender_identity` there is taken from the request envelope, which the
    /// sender writes itself, so checking `daemon_instance_id` against it would look like a check
    /// while refusing nothing.
    async fn report_agent_clone_state(
        &self,
        request: Request<tddy_service::proto::connection::ReportAgentCloneStateRequest>,
    ) -> Result<Response<tddy_service::proto::connection::ReportAgentCloneStateResponse>, Status>
    {
        self.record_rpc_activity();
        let req = request.into_inner();
        self.roster_session_dir(&req.session_token, &req.session_id)?;
        let state = tddy_service::proto::connection::AgentCloneState::try_from(req.clone_state)
            .unwrap_or(tddy_service::proto::connection::AgentCloneState::Unspecified);
        self.session_agent_clones
            .record_report(&crate::session_agent_clone::AgentCloneReport {
                session_id: req.session_id.clone(),
                daemon_instance_id: req.daemon_instance_id.clone(),
                codebase_session_id: req.codebase_session_id.clone(),
                state,
                error: req.clone_error.clone(),
                worktree_path: Some(req.worktree_path.clone())
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from),
                divergences: req.divergences.clone(),
            })?;
        log::info!(
            "ReportAgentCloneState: daemon {} reports session {}'s clone {} as {state:?}{}",
            req.daemon_instance_id,
            req.session_id,
            req.codebase_session_id,
            match req.divergences.len() {
                0 => String::new(),
                n => format!(" with {n} divergence(s)"),
            }
        );
        for divergence in &req.divergences {
            log::error!(
                "session {}'s clone on daemon {} diverged and was reconciled: {divergence}",
                req.session_id,
                req.daemon_instance_id
            );
        }
        let session_dir = self.session_dir_for(&req.session_id)?;
        self.publish_roster_change(&req.session_id, &session_dir)
            .await;
        Ok(Response::new(
            tddy_service::proto::connection::ReportAgentCloneStateResponse {},
        ))
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
                        // FIXME(2026-07-12-fast-session-change): populate from a per-session
                        // traffic meter for GrpcSessionTerminal sessions the daemon owns.
                        // Zero/empty is the honest value until that meter is wired; tddy-coder
                        // sessions report live counters via the participant runtime instead.
                        bytes_in: 0,
                        bytes_out: 0,
                        last_data_received_at: String::new(),
                        // Populated by `apply_session_list_status_to_proto` below from the recipe
                        // manifest; left empty here so the enrichment is the single source of truth.
                        context_docs: Vec::new(),
                        // Populated by `apply_session_list_status_to_proto` below from
                        // Changeset.branch; left empty here so the enrichment is the single source
                        // of truth.
                        branch: String::new(),
                        // A split session's pairing, straight from `.session.yaml`. Empty for a
                        // co-located session, which is every session that does not name another
                        // daemon as its codebase host.
                        codebase_daemon_instance_id: s.codebase_daemon_instance_id,
                        codebase_session_id: s.codebase_session_id,
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
            .map(|p| {
                let repo_root = PathBuf::from(&p.main_repo_path);
                let default_remote =
                    resolve_default_remote_or_empty(&projects_dir, &p.project_id, &repo_root);
                project_entry_from(&p, local_daemon_id.clone(), default_remote)
            })
            .collect();
        log::debug!(
            target: "tddy_daemon::connection_service",
            "list_projects: local_registry_rows={} local_daemon_instance_id={}",
            entries.len(),
            local_daemon_id
        );
        // `local_only` returns just this daemon's rows and skips peer fan-out, breaking the
        // recursion when a peer aggregation call fans out back into `ListProjects`.
        let projects = if req.local_only {
            entries
        } else {
            merge_listed_projects_with_peers(
                &*self.eligible_daemon_source,
                &req.session_token,
                entries,
            )
            .await
        };
        Ok(Response::new(ListProjectsResponse { projects }))
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

        match crate::supervisor_client::spawn_backend_choice(&self.config) {
            crate::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                await_supervised_with_timeout(
                    timeout,
                    "create_project: clone via tddy-supervisor",
                    crate::supervisor_spawn::clone_repo_via_supervisor(
                        &socket_path,
                        &os_user_owned,
                        &git_url_owned,
                        &dest_path,
                    ),
                )
                .await?
            }
            crate::supervisor_client::SpawnBackendChoice::ForkedWorker => {
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
                .await?
            }
        }

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
            remote_name: None,
            host_repo_paths: std::collections::HashMap::new(),
        };
        let repo_root = PathBuf::from(&project.main_repo_path);
        let default_remote =
            resolve_default_remote_or_empty(&projects_dir, &project.project_id, &repo_root);
        let entry = project_entry_from(
            &project,
            local_instance_id_for_config(&self.config),
            default_remote,
        );
        project_storage::add_project(&projects_dir, project)
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateProjectResponse {
            project: Some(entry),
        }))
    }

    async fn add_project_to_host(
        &self,
        request: Request<AddProjectToHostRequest>,
    ) -> Result<Response<AddProjectToHostResponse>, Status> {
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

        // Route to the requested host: local (empty / matching id) or forward to a peer daemon.
        let requested_daemon = req.daemon_instance_id.trim();
        let local_id = local_instance_id_for_config(&self.config);
        let eligible_ids: Vec<String> = self
            .eligible_daemon_source
            .list_eligible_daemons()
            .iter()
            .map(|e| e.instance_id.0.clone())
            .collect();
        let route = crate::livekit_peer_discovery::classify_peer_route(
            &local_id,
            requested_daemon,
            &eligible_ids,
        )
        .map_err(|msg| {
            log::info!("AddProjectToHost: rejected daemon routing: {}", msg);
            Status::failed_precondition(msg)
        })?;

        if let crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id } = route {
            log::info!(
                "AddProjectToHost: forwarding RPC to remote daemon_instance_id={}",
                peer_instance_id
            );
            let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                Status::failed_precondition(
                    "cannot forward AddProjectToHost: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)",
                )
            })?;
            let inner = crate::livekit_peer_discovery::forward_add_project_to_host_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(inner));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;

        // Idempotent: if this host already registers the project_id, return it without re-cloning.
        if let Some(existing) = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
        {
            log::info!(
                "AddProjectToHost: project_id={} already present on this host, returning existing row",
                project_id
            );
            let repo_root = PathBuf::from(&existing.main_repo_path);
            let default_remote =
                resolve_default_remote_or_empty(&projects_dir, &existing.project_id, &repo_root);
            return Ok(Response::new(AddProjectToHostResponse {
                project: Some(project_entry_from(&existing, local_id, default_remote)),
            }));
        }

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

        match crate::supervisor_client::spawn_backend_choice(&self.config) {
            crate::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                await_supervised_with_timeout(
                    timeout,
                    "add_project_to_host: clone via tddy-supervisor",
                    crate::supervisor_spawn::clone_repo_via_supervisor(
                        &socket_path,
                        &os_user_owned,
                        &git_url_owned,
                        &dest_path,
                    ),
                )
                .await?
            }
            crate::supervisor_client::SpawnBackendChoice::ForkedWorker => {
                spawn_blocking_with_timeout(timeout, "add_project_to_host: clone_repo", move || {
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
                .await?
            }
        }

        let main_repo_path = destination
            .canonicalize()
            .unwrap_or(destination)
            .display()
            .to_string();

        let main_branch_ref = {
            let r = req.main_branch_ref.trim();
            (!r.is_empty()).then(|| r.to_string())
        };
        let project = ProjectData {
            project_id: project_id.to_string(),
            name: name.to_string(),
            git_url: git_url.to_string(),
            main_repo_path,
            main_branch_ref,
            remote_name: None,
            host_repo_paths: std::collections::HashMap::new(),
        };
        let (stored, _created) = project_storage::add_or_get_project(&projects_dir, project)
            .map_err(|e| Status::internal(e.to_string()))?;

        let repo_root = PathBuf::from(&stored.main_repo_path);
        let default_remote =
            resolve_default_remote_or_empty(&projects_dir, &stored.project_id, &repo_root);
        Ok(Response::new(AddProjectToHostResponse {
            project: Some(project_entry_from(&stored, local_id, default_remote)),
        }))
    }

    async fn set_project_default_branch(
        &self,
        request: Request<SetProjectDefaultBranchRequest>,
    ) -> Result<Response<SetProjectDefaultBranchResponse>, Status> {
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

        // Route to the requested host: local (empty / matching id) or forward to a peer daemon.
        let requested_daemon = req.daemon_instance_id.trim();
        let local_id = local_instance_id_for_config(&self.config);
        let eligible_ids: Vec<String> = self
            .eligible_daemon_source
            .list_eligible_daemons()
            .iter()
            .map(|e| e.instance_id.0.clone())
            .collect();
        let route = crate::livekit_peer_discovery::classify_peer_route(
            &local_id,
            requested_daemon,
            &eligible_ids,
        )
        .map_err(|msg| {
            log::info!("SetProjectDefaultBranch: rejected daemon routing: {}", msg);
            Status::failed_precondition(msg)
        })?;

        if let crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id } = route {
            log::info!(
                "SetProjectDefaultBranch: forwarding RPC to remote daemon_instance_id={}",
                peer_instance_id
            );
            let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                Status::failed_precondition(
                    "cannot forward SetProjectDefaultBranch: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)",
                )
            })?;
            let inner =
                crate::livekit_peer_discovery::forward_set_project_default_branch_via_livekit(
                    slot,
                    &peer_instance_id,
                    &req,
                )
                .await?;
            return Ok(Response::new(inner));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;

        // Validate the ref shape and project existence up front so the client gets precise codes
        // (invalid_argument / not_found) before any registry mutation.
        tddy_core::validate_chain_pr_integration_base_ref(req.main_branch_ref.trim())
            .map_err(|e| Status::invalid_argument(format!("invalid main_branch_ref: {e}")))?;
        if project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .is_none()
        {
            return Err(Status::not_found("project not found"));
        }

        project_storage::set_project_default_branch(
            &projects_dir,
            project_id,
            req.main_branch_ref.trim(),
        )
        .map_err(|e| Status::internal(e.to_string()))?;

        let stored = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::internal("project vanished after write"))?;
        log::info!(
            "SetProjectDefaultBranch: project_id={} main_branch_ref={}",
            project_id,
            stored.main_branch_ref.as_deref().unwrap_or_default()
        );
        let repo_root = PathBuf::from(&stored.main_repo_path);
        let default_remote =
            resolve_default_remote_or_empty(&projects_dir, &stored.project_id, &repo_root);
        Ok(Response::new(SetProjectDefaultBranchResponse {
            project: Some(project_entry_from(&stored, local_id, default_remote)),
        }))
    }

    async fn start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        self.start_session_core(request.into_inner(), &AttachmentProgressSink::discarding())
            .await
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

        // claude-cli, cursor-cli, and workspace sessions do not use LiveKit — return empty fields immediately.
        if metadata.session_type.as_deref() == Some("claude-cli")
            || metadata.session_type.as_deref() == Some("cursor-cli")
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
                .resume_claude_cli_session(
                    os_user,
                    &req.session_id,
                    session_dir,
                    metadata,
                    &req.session_token,
                )
                .await;
        }

        // --- cursor-cli branch: resume without LiveKit ---
        if metadata.session_type.as_deref() == Some("cursor-cli") {
            return crate::cursor_cli_spawn::resume_cursor_cli_session(
                &self.claude_cli_manager,
                &self.config,
                &req.session_id,
                &session_dir,
                metadata,
            )
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
        // A resumed session's agent is resolved the same way a starting one's is, so an assistant
        // this daemon defined still reaches the child as a def it can build a backend from.
        let resume_agent_def: Option<String> = match resume_agent.as_deref() {
            Some(name) => self
                .agent_def_for_spawn(name, &github_user)
                .await?
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| Status::internal(format!("failed to serialize agent def: {e}")))?,
            None => None,
        };
        let tddy_data_dir_for_spawn = self.tddy_data_dir.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let daemon_log = self.config.log.clone();
        let startup_watch = spawner::StartupWatch::from_config(&self.config);
        let coder_config_path = self.config.coder_config_path.clone();
        let result = match crate::supervisor_client::spawn_backend_choice(&self.config) {
            crate::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                let coder_log_yaml = spawner::coder_log_config_yaml(coder_config_path.as_deref());
                let spawn_req = spawn_worker::build_spawn_request(
                    &os_user,
                    &tool_path,
                    &tddy_data_dir_for_spawn,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: Some(session_id.as_str()),
                        new_session_id: None,
                        project_id: Some(project_id_resume.as_str()).filter(|id| !id.is_empty()),
                        agent: resume_agent.as_deref(),
                        agent_def_json: resume_agent_def.as_deref(),
                        mouse: spawn_mouse,
                        recipe: resume_recipe.as_deref(),
                        stack_parent: None,
                        // Seeding a stack is a creation-time act; a resumed orchestrator already has
                        // whatever stack it was created with.
                        stack_seed_base_session: None,
                        model: None,
                        // TODO(stdio-relay): wire the resume path's reverse channel too.
                        host_session_socket: None,
                    },
                    daemon_log.as_ref(),
                    coder_log_yaml,
                    startup_watch,
                );
                await_supervised_with_timeout(
                    timeout,
                    "ResumeSession: spawn via tddy-supervisor",
                    crate::supervisor_spawn::spawn_session_via_supervisor(&socket_path, &spawn_req),
                )
                .await?
            }
            crate::supervisor_client::SpawnBackendChoice::ForkedWorker => {
                spawn_blocking_with_timeout(timeout, "ResumeSession: spawn", move || {
                    let pid = if project_id_resume.is_empty() {
                        None
                    } else {
                        Some(project_id_resume.as_str())
                    };
                    let coder_log_yaml =
                        spawner::coder_log_config_yaml(coder_config_path.as_deref());
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
                                agent_def_json: resume_agent_def.as_deref(),
                                mouse: spawn_mouse,
                                recipe: resume_recipe.as_deref(),
                                stack_parent: None,
                                // Seeding a stack is a creation-time act; a resumed orchestrator
                                // already has whatever stack it was created with.
                                stack_seed_base_session: None,
                                model: None,
                                // TODO(stdio-relay): wire the resume path's reverse channel too.
                                host_session_socket: None,
                            },
                            daemon_log.as_ref(),
                            coder_log_yaml,
                            startup_watch,
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
                                agent_def_json: resume_agent_def.as_deref(),
                                mouse: spawn_mouse,
                                recipe: resume_recipe.as_deref(),
                                stack_parent: None,
                                // Seeding a stack is a creation-time act; a resumed orchestrator
                                // already has whatever stack it was created with.
                                stack_seed_base_session: None,
                                model: None,
                                // TODO(stdio-relay): wire the resume path's reverse channel too.
                                host_session_socket: None,
                            },
                            child_log_level.as_str(),
                            child_log_format.as_str(),
                            coder_log_yaml.as_deref(),
                            startup_watch,
                        )
                    }
                })
                .await?
            }
        };
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
        // A split session's worktree lives on another daemon, which must lose it first: deleting
        // this side alone would leave a checkout on a host with no session left to reclaim it. A
        // failure to reach that daemon fails the delete rather than silently dropping its half.
        self.delete_paired_codebase_session(&sessions_base, session_id, &req.session_token)
            .await?;
        // Every clone this session's roster created, on every host that built one — including hosts
        // the operator never looked at. Refused rather than continued if one cannot be reached, for
        // the same reason the paired workspace above is: a delete that succeeded here while a
        // checkout survived elsewhere is exactly the silent leak this is for.
        self.tear_down_every_agent_clone(session_id, &req.session_token)
            .await?;
        // Every admission this session minted is void with the session: a mirror that re-admits
        // after the delete must be refused, and `revoke_all_for_session` is the bulk revocation
        // that does it. (Per-daemon revocation on the last detach is in `tear_down_agent_clone`;
        // this is the session-wide sweep that catches admissions whose clones were already gone.)
        let revoked = self.session_admissions.revoke_all_for_session(session_id);
        if revoked > 0 {
            log::info!("revoked {revoked} admission(s) for session {session_id} on session delete");
        }
        // The other direction: this daemon may be *holding* a clone, whose workspace session is the
        // one being deleted. Forgetting it before the directory goes is what stops a tool call
        // arriving a moment later from being served out of a checkout that no longer exists.
        self.hosted_agent_clones.forget_checkout(session_id);
        if let Some(sandbox) = self.sandbox_manager.get(session_id).await {
            sandbox.stop();
        }
        let _ = self.sandbox_manager.remove(session_id).await;
        session_deletion::close_session_room(&self.session_rooms, session_id);
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

    async fn list_worktree_directory(
        &self,
        request: Request<ListWorktreeDirectoryRequest>,
    ) -> Result<Response<ListWorktreeDirectoryResponse>, Status> {
        let req = request.into_inner();
        let worktree_root =
            self.resolve_listed_worktree(&req.session_token, &req.project_id, &req.worktree_path)?;

        let rel_path = req.rel_path.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let join = tokio::task::spawn_blocking(move || {
            crate::worktree_files::list_worktree_directory_entries(&worktree_root, &rel_path)
        });

        let entries = match tokio::time::timeout(timeout, join).await {
            Ok(Ok(Ok(entries))) => entries,
            Ok(Ok(Err(status))) => return Err(status),
            Ok(Err(join_err)) => return Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => {
                return Err(Status::deadline_exceeded(format!(
                "ListWorktreeDirectory: timed out after {}s (spawn_worker_request_timeout_secs)",
                timeout.as_secs()
            )))
            }
        };

        let entries = entries
            .into_iter()
            .map(|e| WorktreeDirEntry {
                name: e.name,
                is_dir: e.is_dir,
                size_bytes: e.size_bytes,
            })
            .collect();
        Ok(Response::new(ListWorktreeDirectoryResponse { entries }))
    }

    async fn read_worktree_file(
        &self,
        request: Request<ReadWorktreeFileRequest>,
    ) -> Result<Response<ReadWorktreeFileResponse>, Status> {
        let req = request.into_inner();
        let worktree_root =
            self.resolve_listed_worktree(&req.session_token, &req.project_id, &req.worktree_path)?;

        let rel_path = req.rel_path.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let join = tokio::task::spawn_blocking(move || {
            crate::worktree_files::read_worktree_file_utf8(&worktree_root, &rel_path)
        });

        let content = match tokio::time::timeout(timeout, join).await {
            Ok(Ok(Ok(content))) => content,
            Ok(Ok(Err(status))) => return Err(status),
            Ok(Err(join_err)) => return Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => {
                return Err(Status::deadline_exceeded(format!(
                    "ReadWorktreeFile: timed out after {}s (spawn_worker_request_timeout_secs)",
                    timeout.as_secs()
                )))
            }
        };

        Ok(Response::new(ReadWorktreeFileResponse {
            content_utf8: content.content_utf8,
            truncated: content.truncated,
            byte_size: content.byte_size,
        }))
    }

    type StreamReadWorktreeFileStream = MpscResultStream<WorktreeFileChunk>;

    /// The byte-exact streaming read — AC15-AC20 of `docs/ft/daemon/session-worktree-sync.md`.
    ///
    /// Same request message, same addressing and the same `resolve_listed_worktree` gate as the
    /// unary `read_worktree_file`; what differs is what comes back. No UTF-8 decoding
    /// exists on this path to fail, and the 1 MiB truncation the unary read applies is gone — the
    /// bound is `max_attachment_bytes` and an over-cap file is **refused before the first frame**
    /// rather than shortened, because a caller cannot tell a truncated file from a whole one once
    /// the frames have started.
    async fn stream_read_worktree_file(
        &self,
        request: Request<ReadWorktreeFileRequest>,
    ) -> Result<Response<Self::StreamReadWorktreeFileStream>, Status> {
        let req = request.into_inner();
        let worktree_root =
            self.resolve_listed_worktree(&req.session_token, &req.project_id, &req.worktree_path)?;

        let rel_path = req.rel_path.clone();
        let max_bytes = self.config.max_attachment_bytes;
        let timeout = self.config.spawn_worker_request_timeout();
        // The listing gate, the size refusal and the read are all filesystem and git work, so they
        // run off the async runtime exactly as the unary read's do. Every one of them can fail the
        // call outright, which is why they happen here rather than inside the stream: a refusal
        // that arrived as a stream item would have to be told apart from a mid-stream read error.
        let join = tokio::task::spawn_blocking(move || {
            crate::worktree_files::read_worktree_file_bytes(&worktree_root, &rel_path, max_bytes)
        });

        let bytes = match tokio::time::timeout(timeout, join).await {
            Ok(Ok(Ok(bytes))) => bytes,
            Ok(Ok(Err(status))) => return Err(status),
            Ok(Err(join_err)) => return Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => {
                return Err(Status::deadline_exceeded(format!(
                "StreamReadWorktreeFile: timed out after {}s (spawn_worker_request_timeout_secs)",
                timeout.as_secs()
            )))
            }
        };

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<WorktreeFileChunk, Status>>();
        for frame in worktree_file_frames(&bytes) {
            // The whole file is already in memory and the channel is unbounded, so this cannot
            // block; a send only fails once the client has gone, and then there is nothing left to
            // send it to.
            if tx.send(Ok(frame)).is_err() {
                break;
            }
        }
        Ok(Response::new(MpscResultStream { rx }))
    }

    type StreamAgentActivityDeltaStream = MpscResultStream<AgentActivityDeltaChunk>;

    /// The tick delta lookup — AC6-AC14 of `docs/ft/daemon/session-worktree-sync.md`.
    ///
    /// The delta lives in the session room's store, which is why this is answered from the room
    /// registry rather than from disk: a patch is a measurement of a live checkout, and the daemon
    /// hosting that room is the only one that took it.
    ///
    /// Authorization comes **first**, before the store is even looked up, for the reason AC14
    /// gives: an unauthenticated caller must not be able to learn which sessions this daemon hosts
    /// by reading apart a `NOT_FOUND` from a `PERMISSION_DENIED`.
    async fn stream_agent_activity_delta(
        &self,
        request: Request<AgentActivityDeltaRequest>,
    ) -> Result<Response<Self::StreamAgentActivityDeltaStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        self.resolve_os_user(&req.session_token)?;

        let call_id = req.call_id.trim();
        if call_id.is_empty() {
            return Err(Status::invalid_argument(
                "call_id is required; there is no whole-worktree delta",
            ));
        }

        // A room this daemon does not host has no measurement of that checkout and never will, so
        // this is an absence rather than a failure — named, so a client can tell "wrong daemon"
        // from "unknown call".
        let store = self
            .session_rooms
            .delta_store(&req.session_id)
            .ok_or_else(|| {
                Status::not_found(format!(
                    "no session room is hosted here for session {}, so it has no deltas",
                    req.session_id
                ))
            })?;

        let scope = match ProtoDeltaScope::try_from(req.scope).unwrap_or(ProtoDeltaScope::Call) {
            ProtoDeltaScope::Call => DeltaScope::Call,
            ProtoDeltaScope::Residual => DeltaScope::Residual,
            ProtoDeltaScope::Tick => DeltaScope::Tick,
        };

        let delta = {
            let store = store
                .lock()
                .map_err(|_| Status::internal("session delta store is poisoned"))?;
            store.delta_for_call(call_id, scope)
        };

        // Both variants are NOT_FOUND and both carry a distinct message, because the client's
        // response differs: an unknown call is a defect to report, an aged-out delta is an ordinary
        // reconcile from the WIP ref. One shared message would make a long mirror's routine
        // recovery indistinguishable from a bug on one side or the other.
        let delta = match delta {
            Ok(delta) => delta,
            Err(DeltaLookupError::UnknownCall { call_id }) => {
                return Err(Status::not_found(format!(
                    "unknown call {call_id}: this daemon has no record of it in session {}",
                    req.session_id
                )))
            }
            Err(DeltaLookupError::AgedOut { call_id, seq }) => {
                return Err(Status::not_found(format!(
                    "delta for call {call_id} (tick {seq}) has aged out of this session's ring; reconcile from the WIP ref"
                )))
            }
        };

        let (tx, rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<AgentActivityDeltaChunk, Status>>();
        for frame in activity_delta_frames(&delta) {
            if tx.send(Ok(frame)).is_err() {
                break;
            }
        }
        Ok(Response::new(MpscResultStream { rx }))
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
            .map(|s| {
                // Overlay the lazy calculator's view of this worktree's size: report its status and
                // (when Cached) prefer its byte count/timestamp over the stats cache's eager walk.
                let size = self.worktree_size_calculator.state(project_id, &s.path);
                let disk_bytes = match size.status {
                    WorktreeSizeStatus::Cached => size.disk_bytes.unwrap_or(s.disk_bytes),
                    _ => s.disk_bytes,
                };
                WorktreeRow {
                    path: s.path.to_string_lossy().to_string(),
                    branch_label: s.branch_label,
                    disk_bytes,
                    changed_files: s.changed_files,
                    lines_added: s.lines_added,
                    lines_removed: s.lines_removed,
                    updated_at_unix_ms: s.updated_at_unix_ms,
                    stale: s.stale,
                    size_status: proto_worktree_size_status(size.status) as i32,
                    size_calculated_at_unix_ms: size.calculated_at_unix_ms.unwrap_or(0),
                }
            })
            .collect();

        Ok(Response::new(ListWorktreesForProjectResponse { worktrees }))
    }

    /// Associated output stream type for [`stream_session_terminal_io`].
    type StreamSessionTerminalIoStream = MpscTerminalOutputStream;

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
            if !first.data.is_empty() {
                let _ = stdin_tx.send(bytes::Bytes::from(first.data));
            }
            // Sandbox bidi path stays live-only (no capture-ring replay on this surface): forward
            // subsequent input chunks to stdin, and bridge the sandbox stdout broadcast into the
            // mpsc-backed stream the tonic/RpcService trait drains.
            let stdin_tx2 = stdin_tx.clone();
            tokio::spawn(async move {
                while let Some(Ok(msg)) = in_stream.next().await {
                    if !msg.data.is_empty() {
                        let _ = stdin_tx2.send(bytes::Bytes::from(msg.data));
                    }
                }
            });
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SessionTerminalOutput>();
            let mut stdout_rx = sandbox.stdout_tx.subscribe();
            let identity = TerminalFrameIdentity::new(&session_id, &terminal_id);
            tokio::spawn(async move {
                loop {
                    match stdout_rx.recv().await {
                        Ok(chunk) => {
                            if tx.send(identity.data_frame(chunk.to_vec())).is_err() {
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

        if !self
            .claude_cli_manager
            .verify_control(&session_id, &first.control_token)
            .await
        {
            return Err(Status::failed_precondition(
                "terminal controlled by another screen",
            ));
        }

        let store = crate::terminal_session_adapter::DaemonTerminalSessionStore::new(Arc::clone(
            &self.claude_cli_manager,
        ));
        let session = store
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

        // Convert the first (open) message and the remaining tonic input stream into the bridge's
        // `terminal_session` proto types so the bidi handler can route through the shared bridge
        // helper (same replay-once / resume-by-offset semantics as the split `StreamTerminalOutput`).
        let first_bridge = to_bridge_terminal_input(&first);
        let in_stream_mapped = tokio_stream::StreamExt::map(in_stream, |item| match item {
            Ok(msg) => Ok(to_bridge_terminal_input(&msg)),
            Err(e) => Err(tddy_rpc::Status::internal(e.to_string())),
        });

        // Per-chunk control-token verifier (the first message was already verified above). The
        // bridge bidi helper calls this on each subsequent input chunk and ends the forwarder when
        // control is lost — matching the daemon's previous per-chunk control-token check.
        let manager_for_verify = Arc::clone(&self.claude_cli_manager);
        let verify_control = move |sid: &str, token: &str| {
            let manager = Arc::clone(&manager_for_verify);
            let sid = sid.to_string();
            let token = token.to_string();
            async move { manager.verify_control(&sid, &token).await }
        };

        let bridge_rx = tddy_terminal_rpc::serve_stream_session_terminal_io_with(
            session,
            session_id,
            first_bridge,
            in_stream_mapped,
            verify_control,
            tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES,
        )
        .await?;

        // Map the bridge's `terminal_session::SessionTerminalOutput` frames (carrying offset
        // metadata) into the daemon's `connection::SessionTerminalOutput` and forward them through
        // the mpsc-backed stream the tonic/RpcService trait drains.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SessionTerminalOutput>();
        tokio::spawn(async move {
            let mut bridge_rx = bridge_rx;
            while let Some(frame) = bridge_rx.recv().await {
                let mapped = match frame {
                    Ok(out) => to_connection_output(out),
                    Err(_) => break,
                };
                if tx.send(mapped).is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(MpscTerminalOutputStream { rx }))
    }

    /// Associated output stream type for [`stream_terminal_output`].
    type StreamTerminalOutputStream = MpscTerminalOutputStream;

    /// Associated output stream type for [`get_terminal_history`].
    type GetTerminalHistoryStream = MpscResultStream<TerminalHistoryChunk>;

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
            // Every frame this stream emits names the session and terminal it came from, so a client
            // rendering another terminal drops it instead of painting it.
            let identity = TerminalFrameIdentity::new(&session_id, &terminal_id);

            // Re-issue the mouse-tracking modes still in effect as the very first frame (not part of
            // the cumulative byte stream, so zeroed offsets), then forward-fill the retained buffer
            // from the appropriate offset. TAIL (first connect) clamps `from_offset = 0` up to the
            // ring's `start_offset`, replaying the full retained buffer; FROM_OFFSET (reconnect)
            // replays only the gap from the client's tracked offset to the tip. Each chunk is tagged
            // with its absolute offsets so the client advances its `currentOffset` to the tip and can
            // resume by offset on the next reconnect — no duplicate replay.
            let is_from_offset =
                req.mode == tddy_service::proto::connection::StreamReplayMode::FromOffset as i32;
            let from_offset = if is_from_offset { req.from_offset } else { 0 };

            let prologue = sandbox
                .capture
                .lock()
                .map(|cap| cap.mode_prologue())
                .unwrap_or_default();
            if !prologue.is_empty() {
                let _ = tx.send(identity.data_frame(prologue));
            }

            // `from_offset` is clamped DOWN to the tip: a client whose cumulative counter drifted
            // ahead of the stream would otherwise be handed its own bogus offset back and would keep
            // asking for bytes the capture will never hold. Exactly one offset-anchored frame is
            // always emitted (an empty one tagged with the tip when there is no gap), so every open
            // SETS the client's cumulative offset instead of leaving it to be inferred from the
            // frames that carry none — matching `tddy_terminal_rpc::bridge`.
            let tip = sandbox
                .capture
                .lock()
                .map(|cap| cap.end_offset())
                .unwrap_or_default();
            let mut cursor = from_offset.min(tip);
            let mut anchored = false;
            loop {
                let chunk = sandbox
                    .capture
                    .lock()
                    .map(|cap| cap.replay_from(cursor, 0, TERMINAL_OUTPUT_FRAME_MAX_BYTES))
                    .unwrap_or_else(|_| tddy_task::CaptureChunk {
                        data: Vec::new(),
                        start_offset: cursor,
                        end_offset: cursor,
                        at_oldest: true,
                        at_end: true,
                    });
                let (end_offset, at_end) = (chunk.end_offset, chunk.at_end);
                if !chunk.data.is_empty() || !anchored {
                    let _ = tx.send(identity.replay_frame(
                        chunk.data,
                        chunk.start_offset,
                        chunk.end_offset,
                        chunk.at_oldest,
                    ));
                    anchored = true;
                }
                cursor = end_offset;
                if at_end {
                    break;
                }
            }

            let mut stdout_rx = sandbox.stdout_tx.subscribe();
            // Sandbox sessions have no unary input-offset ACK source; data frames only.
            tokio::spawn(async move {
                loop {
                    match stdout_rx.recv().await {
                        Ok(chunk) => {
                            if tx.send(identity.data_frame(chunk.to_vec())).is_err() {
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

        let store = crate::terminal_session_adapter::DaemonTerminalSessionStore::new(Arc::clone(
            &self.claude_cli_manager,
        ));
        // Delegate the claude-cli terminal stream to the unified bridge in `tddy-terminal-rpc`, which
        // sends the mode prologue + current last frame first (tagged with absolute offsets), resizes
        // and drains on client dimensions, emits the current ACK up front, then bridges live
        // broadcast output interleaved with ACKs until the child exits. Older history is fetched on
        // demand via `get_terminal_history` as the user scrolls up. The bridge resolves an empty
        // `terminal_id` to the reserved main terminal, matching the daemon's `resolved_terminal_id`.
        let bridge_req = tddy_terminal_rpc::proto::terminal_session::StreamTerminalOutputRequest {
            session_token: req.session_token.clone(),
            session_id: req.session_id.clone(),
            terminal_id: req.terminal_id.clone(),
            initial_cols: req.initial_cols,
            initial_rows: req.initial_rows,
            mode: req.mode,
            from_offset: req.from_offset,
        };
        let bridge_rx = tddy_terminal_rpc::serve_stream_terminal_output_with(
            &store,
            bridge_req,
            tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES,
        )
        .await?;

        // Convert the bridge's `terminal_session::SessionTerminalOutput` frames (which carry the
        // offset metadata) into the daemon's `connection::SessionTerminalOutput` and forward them
        // through the mpsc-backed stream the tonic/RpcService trait drains. The bridge only ever
        // emits `Ok` frames after opening (open errors are surfaced via the `await?` above), so an
        // `Err` here just ends the stream.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SessionTerminalOutput>();
        tokio::spawn(async move {
            let mut bridge_rx = bridge_rx;
            while let Some(frame) = bridge_rx.recv().await {
                let mapped = match frame {
                    Ok(out) => to_connection_output(out),
                    Err(_) => break,
                };
                if tx.send(mapped).is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(MpscTerminalOutputStream { rx }))
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
            let input_offset = req.input_offset;
            handle.send_input(bytes::Bytes::from(req.data), input_offset);
        }
        Ok(Response::new(SendTerminalInputResponse {}))
    }

    /// `GetTerminalHistory`: lazy scroll-up — one chunk of older output ending just before the
    /// request's `before_offset`, then the stream closes. Delegates to the unified bridge in
    /// `tddy-terminal-rpc` over a [`DaemonTerminalSessionStore`]. Sandbox sessions have no capture
    /// ring wired here, so they report `not_found` (the sandbox path streams its own replay).
    async fn get_terminal_history(
        &self,
        request: Request<GetTerminalHistoryRequest>,
    ) -> Result<Response<Self::GetTerminalHistoryStream>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        if self.sandbox_manager.get(&session_id).await.is_some() {
            return Err(Status::not_found("terminal not found or not running"));
        }

        let store = crate::terminal_session_adapter::DaemonTerminalSessionStore::new(Arc::clone(
            &self.claude_cli_manager,
        ));
        let bridge_req = tddy_terminal_rpc::proto::terminal_session::GetTerminalHistoryRequest {
            session_token: req.session_token.clone(),
            session_id: req.session_id.clone(),
            terminal_id: req.terminal_id.clone(),
            from_offset: req.from_offset,
            until_offset: req.until_offset,
            max_bytes: req.max_bytes,
        };
        let bridge_rx = tddy_terminal_rpc::serve_get_terminal_history_with(
            &store,
            bridge_req,
            tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES,
        )
        .await?;

        let (tx, rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<TerminalHistoryChunk, Status>>();
        tokio::spawn(async move {
            let mut bridge_rx = bridge_rx;
            while let Some(frame) = bridge_rx.recv().await {
                let mapped = frame.map(|chunk| TerminalHistoryChunk {
                    data: chunk.data,
                    start_offset: chunk.start_offset,
                    end_offset: chunk.end_offset,
                    at_oldest: chunk.at_oldest,
                    at_end: chunk.at_end,
                });
                if tx.send(mapped).is_err() {
                    break;
                }
            }
        });
        Ok(Response::new(MpscResultStream { rx }))
    }

    async fn start_terminal_session(
        &self,
        request: Request<StartTerminalSessionRequest>,
    ) -> Result<Response<StartTerminalSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
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

        // The Bash tool is built-in: the target user's passwd login shell (not the daemon's own
        // `$SHELL`, which under systemd/nix is not the user's interactive shell), falling back to
        // `$SHELL`, then /bin/bash.
        let shell = crate::pty_runtime::login_shell_for_os_user(os_user)
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/bash".to_string());

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

        // Before the checkout goes, not after: a session room measures its directory on an
        // interval, so one still hosted for this path would shell out to git in a directory that no
        // longer exists — warning at the poll rate for the life of the daemon. This RPC removes a
        // checkout by path and never learns a session id, so the registry is asked by path.
        self.session_rooms.close_for_worktree(&worktree_path);

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

    async fn clean_worktree(
        &self,
        request: Request<CleanWorktreeRequest>,
    ) -> Result<Response<CleanWorktreeResponse>, Status> {
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
            worktrees::clean_worktree_under_repo(&repo_blocking, &wt_blocking)
        });

        match tokio::time::timeout(timeout, join).await {
            Ok(Ok(Ok(()))) => {
                self.worktree_stats_cache.invalidate_project(project_id);
                Ok(Response::new(CleanWorktreeResponse {
                    ok: true,
                    message: String::new(),
                }))
            }
            Ok(Ok(Err(e))) => Err(map_clean_worktree_error(e)),
            Ok(Err(join_err)) => Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => Err(Status::deadline_exceeded(format!(
                "CleanWorktree: timed out after {}s (spawn_worker_request_timeout_secs)",
                timeout.as_secs()
            ))),
        }
    }

    async fn restore_session_worktree(
        &self,
        request: Request<RestoreSessionWorktreeRequest>,
    ) -> Result<Response<RestoreSessionWorktreeResponse>, Status> {
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
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }
        validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

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

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions base"))?;
        let session_dir = unified_session_dir_path(&sessions_base, session_id);

        let repo_blocking = main_repo.clone();
        let session_dir_blocking = session_dir.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let join = tokio::task::spawn_blocking(move || {
            let base_ref = tddy_core::resolve_persisted_worktree_integration_base_for_session(
                &session_dir_blocking,
                &repo_blocking,
            )?;
            tddy_core::setup_worktree_for_session_with_integration_base(
                &repo_blocking,
                &session_dir_blocking,
                &base_ref,
            )
        });

        match tokio::time::timeout(timeout, join).await {
            Ok(Ok(Ok(path))) => {
                self.worktree_stats_cache.invalidate_project(project_id);
                Ok(Response::new(RestoreSessionWorktreeResponse {
                    ok: true,
                    message: String::new(),
                    worktree_path: path.to_string_lossy().into_owned(),
                }))
            }
            Ok(Ok(Err(e))) => Err(Status::internal(e)),
            Ok(Err(join_err)) => Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => Err(Status::deadline_exceeded(format!(
                "RestoreSessionWorktree: timed out after {}s (spawn_worker_request_timeout_secs)",
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
        let remote = project_storage::effective_remote_name_for_project(
            &projects_dir,
            project_id,
            &repo_root,
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        let remote_for_closure = remote.clone();
        let branches = spawn_blocking_with_timeout(
            timeout,
            "ListProjectBranches: git remote refs",
            move || {
                tddy_core::list_recent_remote_branches(
                    &repo_root,
                    &remote_for_closure,
                    BRANCH_LIST_LIMIT,
                )
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

        Ok(Response::new(ListProjectBranchesResponse {
            branches,
            default_remote: remote,
        }))
    }

    async fn execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<ExecuteToolResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay (which has no local sessions) can forward.
        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_exec_tool_route("ExecuteTool", &req.daemon_instance_id)?
        {
            let slot = self.common_room_slot("ExecuteTool")?;
            let out = crate::livekit_peer_discovery::forward_to_peer(
                slot,
                &peer_instance_id,
                "connection.ConnectionService",
                "ExecuteTool",
                req.encode_to_vec(),
            )
            .await?;
            let inner = ExecuteToolResponse::decode(out.as_slice())
                .map_err(|e| Status::internal(format!("decode ExecuteToolResponse: {e}")))?;
            return Ok(Response::new(inner));
        }

        // Auth before *any* worktree is chosen, because the hosted-clone branch below chooses one
        // that is not this daemon's and proxies its mutations under the clone's own credential.
        self.authorize_exec_tool_caller(&req)?;

        // A session this daemon holds an *agent clone* for lives on another daemon, so the ordinary
        // "resolve the worktree from my own sessions base" would find nothing. Checked before that
        // resolution rather than after it, so the read/write split is what answers rather than a
        // not-found for a session that legitimately is not here.
        if let Some(clone) = self.hosted_clone_for(&req.session_id) {
            reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
            return Ok(Response::new(
                self.run_hosted_clone_tool(&req, &clone).await,
            ));
        }

        let (sessions_base, worktree_root) = self.resolve_exec_tool_worktree(&req)?;
        reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
        let response = self
            .run_exec_tool_locally(&req, &sessions_base, &worktree_root)
            .await;
        Ok(Response::new(response))
    }

    /// Associated output stream type for [`stream_execute_tool`].
    type StreamExecuteToolStream = MpscResultStream<ExecuteToolChunk>;

    /// Server-streaming sibling of [`Self::execute_tool`], carrying the same result in bounded
    /// frames.
    ///
    /// The unary call returns `result_json` as one string; over LiveKit anything past
    /// `MAX_CHUNK_FRAME_BYTES` is chunk-framed, and one lost chunk frame wedges the call with no
    /// error at all (`docs/ft/coder/rpc-multi-transport.md`). A `Read` of a large file crosses that
    /// on day one of a split session, so the split path streams instead — routing, auth and worktree
    /// resolution are shared with the unary handler so the two cannot drift.
    async fn stream_execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<Self::StreamExecuteToolStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_exec_tool_route("StreamExecuteTool", &req.daemon_instance_id)?
        {
            let slot = self.common_room_slot("StreamExecuteTool")?;
            // A forwarded stream that stalls terminates as an *error*, so a truncated tool result
            // can never reach the caller looking complete.
            let rx = crate::livekit_peer_discovery::forward_stream_execute_tool_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(MpscResultStream { rx }));
        }

        // See the unary handler: auth first, because the hosted-clone branch resolves no worktree of
        // this daemon's and would otherwise be reachable with no credential at all.
        self.authorize_exec_tool_caller(&req)?;

        // A session this daemon holds an agent clone for is served by the read/write split, from a
        // checkout that is not in this daemon's own sessions base.
        let response = match self.hosted_clone_for(&req.session_id) {
            Some(clone) => {
                reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
                self.run_hosted_clone_tool(&req, &clone).await
            }
            None => {
                let (sessions_base, worktree_root) = self.resolve_exec_tool_worktree(&req)?;
                reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
                self.run_exec_tool_locally(&req, &sessions_base, &worktree_root)
                    .await
            }
        };

        // The result is already complete in memory, so every frame can be queued now: the stream
        // exists to bound each frame's size, not to interleave with the tool's execution.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<ExecuteToolChunk, Status>>();
        for frame in exec_tool_result_frames(response) {
            if tx.send(Ok(frame)).is_err() {
                break;
            }
        }
        Ok(Response::new(MpscResultStream { rx }))
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
            tools: tool_engine::tool_catalog()
                .into_iter()
                .map(|t| tddy_service::proto::connection::ToolDef {
                    name: t.name,
                    description: t.description,
                    input_schema_json: t.input_schema_json,
                })
                .collect(),
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

        // claude-cli and cursor-cli sessions support hook status reporting.
        let session_type = meta.session_type.as_deref().unwrap_or("");
        if session_type != "claude-cli" && session_type != "cursor-cli" {
            return Err(Status::failed_precondition(
                "session_type is not claude-cli or cursor-cli",
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
                &telegram.config,
                &*telegram.sender,
                &req.session_id,
                &req.status,
            )
            .await;
        }

        Ok(Response::new(ReportSessionStatusResponse { ok: true }))
    }

    async fn report_agent_activity(
        &self,
        request: Request<ReportAgentActivityRequest>,
    ) -> Result<Response<ReportAgentActivityResponse>, Status> {
        let req = request.into_inner();

        // Validate session_id segment to prevent path traversal.
        tddy_core::validate_session_id_segment(&req.session_id)
            .map_err(|_| Status::invalid_argument("invalid session_id"))?;

        // Resolve sessions_base from os_user (no web session token available for hooks).
        let sessions_base = crate::user_sessions_path::sessions_base_for_user(
            &req.os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::not_found("unknown os_user or sessions_base not found"))?;

        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        // Read session metadata — not found if the directory/yaml doesn't exist.
        let meta = tddy_core::read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        // Validate hook_token (local-process comparison; the token is a per-session secret).
        let stored_token = meta.hook_token.as_deref().unwrap_or("");
        if stored_token != req.hook_token {
            return Err(Status::permission_denied("invalid hook_token"));
        }

        // AC1/AC2 of `docs/ft/daemon/session-worktree-sync.md`: the record names the commit it was
        // made against and the paths it declared, so a consumer holding a patch can place it. The
        // HEAD is read from the filesystem rather than by spawning `git rev-parse` — an agent makes
        // a great many tool calls, and a subprocess on each would be paid on every one of them.
        //
        // A session with no checkout on this host stamps neither: `read_head_commit` returns an
        // empty string when HEAD cannot be resolved, and a path has nothing to be relative to. That
        // is the honest answer AC1 asks for, and the reason no sha is invented in its place.
        let worktree_root = meta.repo_path.as_deref().map(PathBuf::from);
        let head_commit = worktree_root
            .as_deref()
            .map(tddy_core::git_head::read_head_commit)
            .unwrap_or_default();
        let input = tddy_core::agent_activity::parse_activity_json(&req.input_json);
        let changed_paths = worktree_root
            .as_deref()
            .map(|root| tddy_core::agent_activity::declared_paths(&req.tool_name, &input, root))
            .unwrap_or_default();

        let record = match req.event.as_str() {
            "PreToolUse" => {
                // A tool call started: mint a call_id, remember it so the paired PostToolUse can
                // reuse it, and append the `running` row.
                let call_id = Uuid::new_v4().to_string();
                self.agent_activity_hub
                    .push_pending(&req.session_id, &call_id);
                tddy_core::agent_activity::AgentActivityRecord {
                    call_id,
                    tool_name: req.tool_name,
                    input,
                    status: tddy_core::agent_activity::STATUS_RUNNING.to_string(),
                    result: serde_json::Value::Null,
                    error_message: String::new(),
                    started_unix_ms: now_unix_ms(),
                    completed_unix_ms: 0,
                    source: "claude-cli".to_string(),
                    head_commit,
                    // The tick that covers this call has not been measured yet; the poll loop
                    // attributes it when it runs. `0` is the wire's "no tick has covered it yet".
                    activity_seq: 0,
                    changed_paths,
                }
            }
            "PostToolUse" => {
                // The tool call finished: pair with the most-recent pending call_id (fresh id when
                // none is outstanding, e.g. a hook restart), and append the terminal row.
                let call_id = self
                    .agent_activity_hub
                    .pop_pending(&req.session_id)
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                let status = if req.is_error {
                    tddy_core::agent_activity::STATUS_ERROR
                } else {
                    tddy_core::agent_activity::STATUS_COMPLETED
                };
                tddy_core::agent_activity::AgentActivityRecord {
                    call_id,
                    tool_name: req.tool_name,
                    input,
                    status: status.to_string(),
                    result: tddy_core::agent_activity::parse_activity_json(&req.result_json),
                    error_message: req.error_message,
                    started_unix_ms: 0,
                    completed_unix_ms: now_unix_ms(),
                    source: "claude-cli".to_string(),
                    head_commit,
                    // As on the `running` row: the covering tick is the poll loop's to attribute.
                    activity_seq: 0,
                    changed_paths,
                }
            }
            other => {
                return Err(Status::invalid_argument(format!(
                    "unknown event: {other} (expected PreToolUse or PostToolUse)"
                )));
            }
        };

        // The durable log is the source of truth; a write failure must not fail the hook call.
        if let Err(e) = tddy_core::agent_activity::append_agent_activity(&session_dir, &record) {
            log::warn!(
                "agent_activity: failed to persist {} for session {}: {}",
                req.event,
                req.session_id,
                e
            );
        }
        self.agent_activity_hub.publish(&req.session_id, record);
        // The record is **not** broadcast into the session room from here, deliberately.
        //
        // A record announced at this point names a tick nothing has measured yet: its
        // `activity_seq` is still `0` and the delta covering its files is produced by the next poll
        // tick, so a participant that reacted to it and asked for the call's delta would be told
        // `UnknownCall` — an announcement that arrives before the thing it announces.
        //
        // The poll loop is the single broadcaster instead, tailing `agent-activity.jsonl`, which is
        // also what makes cursor-cli and tool sessions visible: their agents never call this RPC at
        // all, and a room fed only from here would carry claude-cli activity and nothing else.
        Ok(Response::new(ReportAgentActivityResponse { ok: true }))
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
            // Pinned to what the launcher did before any of this was configurable, because
            // nothing here knows the demo image's architecture — `demo-plan.md` names a
            // build target, and `tddy-build-qemu` produces x86_64 images. Deriving these
            // from the *host* would run an aarch64 emulator against an x86_64 image on an
            // Apple Silicon machine, and `virt` has no BIOS for it to fall back to.
            // TCG likewise matches the previous behaviour and avoids making the demo path
            // newly dependent on the daemon user's access to /dev/kvm.
            arch: tddy_vm::VmArch::X86_64,
            accel: tddy_vm::VmAccel::Tcg,
            // The resources the launcher hard-coded before they were configurable.
            memory: "512M".to_string(),
            cpus: 1,
            // The demo images boot through their own BIOS; they carry no cloud-init seed
            // and share nothing from the host.
            firmware: None,
            login: tddy_vm::VmLogin {
                username: "root".to_string(),
                private_key_path: None,
            },
            seed_iso: None,
            nine_p_shares: vec![],
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

    // --- agent activity ---

    type StreamSessionActivityStream = MpscAgentActivityStream;

    /// Stream a session's agent activity: replay the persisted `agent-activity.jsonl` snapshot,
    /// then relay live records published to the hub for this session.
    async fn stream_session_activity(
        &self,
        request: Request<StreamSessionActivityRequest>,
    ) -> Result<Response<Self::StreamSessionActivityStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay can forward. A request addressed to a remote daemon
        // is rejected rather than silently served from the local (wrong) log.
        // TODO(agent-activity): forward StreamSessionActivity to a peer daemon over
        // `forward_server_stream_to_peer`. The primitive exists and carries an idle deadline sized
        // for a short-lived stream; this one is long-lived and open-ended, so migrating it needs a
        // keepalive frame (or a per-call deadline) first — otherwise an idle session's activity
        // stream would be terminated as a stalled peer.
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
                    log::info!("StreamSessionActivity: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    return Err(Status::unimplemented(format!(
                        "StreamSessionActivity forwarding to remote daemon_instance_id={peer_instance_id} is not supported yet"
                    )));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Local) => {
                    // Fall through to local execution below.
                }
            }
        }

        // Authenticate caller (same path as list_session_tool_calls).
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ProtoAgentActivityRecord>();

        // Snapshot-then-live (the default and proto3 zero value) replays the coalesced on-disk
        // records first, then relays everything subsequently published to the hub for this
        // session. Live-only skips the snapshot entirely and carries only records published after
        // subscribe.
        let mode = StreamMode::try_from(req.mode).unwrap_or(StreamMode::SnapshotThenLive);
        if mode == StreamMode::SnapshotThenLive {
            let snapshot =
                tddy_core::agent_activity::read_agent_activity(&session_dir).unwrap_or_default();
            for record in snapshot {
                if tx
                    .send(tddy_service::agent_activity_to_proto(record))
                    .is_err()
                {
                    // Receiver already gone — return an empty live stream that terminates immediately.
                    return Ok(Response::new(MpscAgentActivityStream { rx }));
                }
            }
        }

        let broadcast_rx = self.agent_activity_hub.subscribe(&req.session_id);
        tokio::spawn(relay_agent_activity(broadcast_rx, tx));

        Ok(Response::new(MpscAgentActivityStream { rx }))
    }

    // --- ACP transcript replay ---

    type StreamAcpReplayStream = MpscAcpReplayStream;

    /// Stream a session's read-only ACP transcript: replay the session's resolved transcript
    /// snapshot (`acp-transcript.jsonl` merged with the durable `agent-activity.jsonl` — see
    /// [`tddy_service::acp_replay::read_session_transcript`]), then relay live agent-activity records
    /// (mapped to ACP `tool_call` frames) published to the hub for this session. Mirrors
    /// [`stream_session_activity`] — same routing, auth, and [`StreamMode`] semantics.
    async fn stream_acp_replay(
        &self,
        request: Request<StreamAcpReplayRequest>,
    ) -> Result<Response<Self::StreamAcpReplayStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay can forward. A request addressed to a remote daemon
        // is rejected rather than silently served from the local (wrong) transcript.
        // TODO(acp-replay): forward StreamAcpReplay to a peer daemon over
        // `forward_server_stream_to_peer`, blocked on the same keepalive gap as
        // `stream_session_activity` above — this stream stays open for a session's whole life, and
        // the primitive's idle deadline is sized for a short-lived one.
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
                    log::info!("StreamAcpReplay: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    return Err(Status::unimplemented(format!(
                        "StreamAcpReplay forwarding to remote daemon_instance_id={peer_instance_id} is not supported yet"
                    )));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Local) => {
                    // Fall through to local execution below.
                }
            }
        }

        // Authenticate caller (same path as stream_session_activity).
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<AcpReplayFrame>();

        // Snapshot-then-live (the default and proto3 zero value) replays the persisted transcript
        // first, then relays everything subsequently published to the hub for this session.
        // Live-only skips the snapshot entirely and carries only frames produced after subscribe.
        let mode = StreamMode::try_from(req.mode).unwrap_or(StreamMode::SnapshotThenLive);

        // Count-first mode emits only the running count of persisted transcript frames — one frame
        // now with the current count, then a fresh count each time a record is published — with no
        // transcript payload. It never replays the snapshot itself.
        if mode == StreamMode::CountThenLive {
            let snapshot =
                tddy_service::acp_replay::read_session_transcript(&session_dir).unwrap_or_default();
            let count = tddy_service::acp_replay::count_activity_entries(&snapshot);
            let seen_ids = tddy_service::acp_replay::tool_call_ids(&snapshot);
            if tx
                .send(AcpReplayFrame {
                    acp_agent_message: Vec::new(),
                    activity_count: count,
                    // A count frame carries no transcript payload, so it has no position.
                    seq: 0,
                })
                .is_err()
            {
                // Receiver already gone — return an empty live stream that terminates immediately.
                return Ok(Response::new(MpscAcpReplayStream { rx }));
            }
            let broadcast_rx = self.agent_activity_hub.subscribe(&req.session_id);
            tokio::spawn(relay_acp_replay_count(broadcast_rx, tx, count, seen_ids));
            return Ok(Response::new(MpscAcpReplayStream { rx }));
        }

        // The resolved transcript is what every position refers to: the replayed frames index into
        // it, and the live tail continues its numbering from the end of it. Live-only replays none
        // of it but still needs its length, so a live frame's `seq` means the same thing there.
        let snapshot =
            tddy_service::acp_replay::read_session_transcript(&session_dir).unwrap_or_default();

        // Which slice of the transcript is replayed on subscribe, and where in the transcript that
        // slice starts: all of it (snapshot-then-live, the proto3 default), its newest page only
        // (tail-then-live), or none of it (live-only).
        let (first_seq, replayed): (u64, &[tddy_service::proto::acp::AcpAgentMessage]) = match mode
        {
            StreamMode::SnapshotThenLive => (0, &snapshot),
            StreamMode::TailThenLive => {
                let page = tddy_service::acp_replay::tail_page(
                    &snapshot,
                    usize::try_from(req.page_size).unwrap_or(usize::MAX),
                );
                (page.first_seq, page.frames)
            }
            _ => (0, &[]),
        };
        for (offset, frame) in replayed.iter().enumerate() {
            if tx
                .send(acp_replay_frame(frame, first_seq + offset as u64))
                .is_err()
            {
                // Receiver already gone — return an empty live stream that terminates immediately.
                return Ok(Response::new(MpscAcpReplayStream { rx }));
            }
        }

        let broadcast_rx = self.agent_activity_hub.subscribe(&req.session_id);
        tokio::spawn(relay_acp_replay(
            broadcast_rx,
            tx,
            snapshot.len() as u64,
            seq_by_tool_call(&snapshot),
        ));

        Ok(Response::new(MpscAcpReplayStream { rx }))
    }

    /// Return one tool call's full `raw_input`/`raw_output` from the session's coalesced transcript
    /// (the bodies `stream_acp_replay` strips out). Mirrors `stream_acp_replay`'s routing/auth and
    /// maps an unknown `tool_call_id` to `NOT_FOUND`.
    async fn get_acp_tool_call_detail(
        &self,
        request: Request<GetAcpToolCallDetailRequest>,
    ) -> Result<Response<GetAcpToolCallDetailResponse>, Status> {
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
                    log::info!("GetAcpToolCallDetail: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    log::info!(
                        "GetAcpToolCallDetail: forwarding RPC to remote daemon_instance_id={}",
                        peer_instance_id
                    );
                    let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                        Status::failed_precondition(
                            "cannot forward GetAcpToolCallDetail: this process has no LiveKit common-room connection",
                        )
                    })?;
                    let body = req.encode_to_vec();
                    let out = crate::livekit_peer_discovery::forward_to_peer(
                        slot,
                        &peer_instance_id,
                        "connection.ConnectionService",
                        "GetAcpToolCallDetail",
                        body,
                    )
                    .await?;
                    let inner =
                        GetAcpToolCallDetailResponse::decode(out.as_slice()).map_err(|e| {
                            Status::internal(format!("decode GetAcpToolCallDetailResponse: {e}"))
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

        // Resolve the session dir.
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        let detail = tddy_service::acp_replay::tool_call_detail(&session_dir, &req.tool_call_id)
            .map_err(|e| Status::internal(format!("read transcript: {e}")))?;
        match detail {
            None => Err(Status::not_found(format!(
                "no tool call with id {} in session {}",
                req.tool_call_id, req.session_id
            ))),
            Some(detail) => Ok(Response::new(GetAcpToolCallDetailResponse {
                raw_input: detail.raw_input,
                raw_output: detail.raw_output,
            })),
        }
    }

    /// Return one page of transcript frames strictly older than `before_seq` — the reverse cursor a
    /// tail-first replay pages backwards with. Mirrors [`get_acp_tool_call_detail`]'s routing (it
    /// peer-forwards, unlike the streaming modes) and `stream_acp_replay`'s auth, and applies the
    /// same `strip_tool_body` seam the replay stream does: a paged frame is not a back door to the
    /// bodies.
    async fn get_acp_replay_page(
        &self,
        request: Request<GetAcpReplayPageRequest>,
    ) -> Result<Response<GetAcpReplayPageResponse>, Status> {
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
                    log::info!("GetAcpReplayPage: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    log::info!(
                        "GetAcpReplayPage: forwarding RPC to remote daemon_instance_id={}",
                        peer_instance_id
                    );
                    let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                        Status::failed_precondition(
                            "cannot forward GetAcpReplayPage: this process has no LiveKit common-room connection",
                        )
                    })?;
                    let body = req.encode_to_vec();
                    let out = crate::livekit_peer_discovery::forward_to_peer(
                        slot,
                        &peer_instance_id,
                        "connection.ConnectionService",
                        "GetAcpReplayPage",
                        body,
                    )
                    .await?;
                    let inner = GetAcpReplayPageResponse::decode(out.as_slice()).map_err(|e| {
                        Status::internal(format!("decode GetAcpReplayPageResponse: {e}"))
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

        // Resolve the session dir.
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        // A transcript that cannot be read is an error, never an empty page: an empty page means
        // "you have reached the head", and a reader told that stops paging for good.
        let transcript = tddy_service::acp_replay::read_session_transcript(&session_dir)
            .map_err(|e| Status::internal(format!("read transcript: {e}")))?;
        let page = tddy_service::acp_replay::page_before(
            &transcript,
            req.before_seq,
            usize::try_from(req.page_size).unwrap_or(usize::MAX),
        );

        Ok(Response::new(GetAcpReplayPageResponse {
            frames: page
                .frames
                .iter()
                .map(|frame| tddy_service::acp_replay::strip_tool_body(frame).encode_to_vec())
                .collect(),
            first_seq: page.first_seq,
            at_oldest: page.at_oldest,
        }))
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

    // --- PR-Stack Chat Screen: manually adding a planned PR ---

    /// Append a manually-created planned PR to a "pr-stack" orchestrator session's stack,
    /// choosing its ancestors from the already-planned nodes. See
    /// `tddy_workflow_recipes::pr_stack::add_planned_pr_node`.
    async fn add_planned_pr(
        &self,
        request: Request<AddPlannedPrRequest>,
    ) -> Result<Response<AddPlannedPrResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "AddPlannedPr: session_id={} title={:?}",
            req.session_id.trim(),
            req.title
        );
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.title.trim().is_empty() {
            return Err(Status::invalid_argument("title is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        let branch_suggestion =
            (!req.branch_suggestion.trim().is_empty()).then(|| req.branch_suggestion.clone());
        let child_recipe = (!req.child_recipe.trim().is_empty()).then(|| req.child_recipe.clone());

        // The appended node, so the response can *name* what this call created. The caller must not
        // infer it by diffing the returned plan: the orchestrator agent appends nodes to the same
        // stack, so a plan can come back holding several nodes the caller has never seen.
        let added = tddy_workflow_recipes::pr_stack::add_planned_pr_node(
            &session_dir,
            tddy_workflow_recipes::pr_stack::AddPlannedPrInput {
                title: req.title.clone(),
                description: req.description.clone(),
                branch_suggestion,
                parents: req.parents.clone(),
                child_recipe,
            },
        )
        .map_err(Status::invalid_argument)?;

        // Re-read the just-updated changeset and reuse the same serializer as `ListSessions`
        // enrichment so the response's `stack_plan_json` is byte-for-byte the same wire shape
        // `PrStackScreen`'s `parseStackPlan` already knows how to read.
        let changeset =
            tddy_core::read_changeset(&session_dir).map_err(|e| Status::internal(e.to_string()))?;
        let stack_plan_json = session_list_enrichment::stack_plan_json_for_changeset(&changeset);

        log::info!(
            "AddPlannedPr: success session_id={} node_id={} title={:?}",
            req.session_id.trim(),
            added.node_id,
            req.title
        );
        Ok(Response::new(AddPlannedPrResponse {
            stack_plan_json,
            node_id: added.node_id,
        }))
    }

    async fn get_pr_status(
        &self,
        request: Request<GetPrStatusRequest>,
    ) -> Result<Response<GetPrStatusResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.branch.trim().is_empty() {
            return Err(Status::invalid_argument("branch is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        // An orchestrator never gets a worktree, so its `changeset.yaml` records no checkout and only
        // `.session.yaml` names the one it plans over. `pr_status_for_caller` resolves the GitHub
        // namespace from that checkout's remote and reports a lookup it cannot perform — including
        // one over an unknown checkout — as unavailable rather than failing this call.
        let repo_root = tddy_core::repo_root_for_session(&session_dir);

        let status = self
            .pr_status_for_caller(&github_user, repo_root.as_deref(), req.branch.trim())
            .await;
        Ok(Response::new(GetPrStatusResponse {
            status: Some(status),
        }))
    }

    async fn get_worktree_snapshot(
        &self,
        request: Request<GetWorktreeSnapshotRequest>,
    ) -> Result<Response<GetWorktreeSnapshotResponse>, Status> {
        let req = request.into_inner();

        // Routed exactly like ExecuteTool, and for the same reason: the caller names the daemon it
        // believes holds the checkout, and a session room on the agent's daemon polls a remote
        // checkout by addressing the codebase daemon. Reusing that classifier keeps one answer to
        // "which daemon owns this session's files".
        match self.classify_exec_tool_route("GetWorktreeSnapshot", &req.daemon_instance_id)? {
            PeerRoute::Local => {}
            PeerRoute::Forward { peer_instance_id } => {
                let slot = self.common_room_slot("GetWorktreeSnapshot")?;
                let out = crate::livekit_peer_discovery::forward_to_peer(
                    slot,
                    &peer_instance_id,
                    "connection.ConnectionService",
                    "GetWorktreeSnapshot",
                    req.encode_to_vec(),
                )
                .await?;
                let inner = GetWorktreeSnapshotResponse::decode(out.as_slice())
                    .map_err(|e| Status::internal(format!("decoding peer snapshot: {e}")))?;
                return Ok(Response::new(inner));
            }
        }

        // The measurement is assembled here, where the files are, and shells out to git — so it
        // runs on the blocking pool under the same budget a local poll uses. A caller that gave up
        // waiting is a caller whose next tick will ask again.
        let (sessions_base, worktree_root) =
            self.resolve_exec_tool_worktree(&ExecuteToolRequest {
                session_token: req.session_token.clone(),
                session_id: req.session_id.clone(),
                tool_name: "GetWorktreeSnapshot".to_string(),
                args_json: String::new(),
                daemon_instance_id: req.daemon_instance_id.clone(),
            })?;

        let budget = self.config.session_room_git_timeout();
        let measured_root = worktree_root.clone();
        let snapshot = tokio::task::spawn_blocking(move || {
            crate::session_room::snapshot_worktree_within(&measured_root, budget)
        })
        .await
        .map_err(|e| Status::internal(format!("measuring {worktree_root:?} panicked: {e}")))?;

        let session_dir =
            tddy_core::session_lifecycle::unified_session_dir_path(&sessions_base, &req.session_id);
        let attachments = crate::session_attachments::list_session_attachments(&session_dir)
            .into_iter()
            .map(|a| a.basename)
            .collect();

        Ok(Response::new(GetWorktreeSnapshotResponse {
            head_commit: snapshot.head_commit,
            branch: snapshot.branch,
            changed_paths: snapshot.changed_paths,
            changed_files: snapshot.changed_files,
            lines_added: snapshot.lines_added,
            lines_removed: snapshot.lines_removed,
            untracked_files: snapshot.untracked_files,
            attachments,
        }))
    }

    async fn query_branch(
        &self,
        request: Request<QueryBranchRequest>,
    ) -> Result<Response<QueryBranchResponse>, Status> {
        use tddy_service::proto::connection::{
            BranchRemote, BranchResolution, BranchSession, BranchWorktree,
        };

        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.branch.trim().is_empty() {
            return Err(Status::invalid_argument("branch is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        // `None` when nothing in the session directory names a checkout: every repo-derived leg below
        // then reads as absent or unavailable, never as a confident answer about a repo we don't know.
        let repo_root = tddy_core::repo_root_for_session(&session_dir);
        let branch = req.branch.trim().to_string();

        // Session — the session that owns the branch, by the one rule `branch_owner` holds for every
        // surface that asks (prefer active, then most-recently-updated).
        let branch_for_scan = branch.clone();
        let sessions_base_for_scan = sessions_base.clone();
        let session = spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "QueryBranch: scan sessions by branch",
            move || {
                crate::branch_owner::find_session_owning_branch(
                    &sessions_base_for_scan,
                    &branch_for_scan,
                )
            },
        )
        .await?;
        let session = match session {
            Some(s) => BranchSession {
                exists: true,
                session_id: s.session_id,
                is_active: s.is_active,
                status: s.status,
            },
            None => BranchSession::default(),
        };

        // Worktree — the on-disk worktree checked out for the branch (non-erroring), and whether it
        // holds outstanding work. Dirtiness is deliberately *not* cached with the base comparison
        // below: it is not a function of the two commits, and an operator's edit must show up on the
        // next tick.
        //
        // Both halves are git subprocesses — a `worktree list` walk and a `status --porcelain` — so
        // they run on the blocking pool like every other leg. A timeout degrades this leg to "no
        // worktree" rather than failing the call, which is the same contract the other four keep.
        let worktree_repo_root = repo_root.clone();
        let branch_for_worktree = branch.clone();
        let worktree = spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "QueryBranch: read the branch's worktree",
            move || {
                Ok(worktree_leg(
                    worktree_repo_root.as_deref(),
                    &branch_for_worktree,
                ))
            },
        )
        .await
        .unwrap_or_else(|status| {
            log::warn!(
                "QueryBranch: the worktree leg did not complete: {}",
                status.message()
            );
            BranchWorktree::default()
        });

        // Remote — `origin/<branch>` in the orchestrator's repo, which is what a descendant's
        // worktree is created from. Only as fresh as the last fetch, so it can delay a spawn but
        // never permit one that would fail inside `git fetch`.
        //
        // Shells out to `git rev-parse`, so it goes to the blocking pool like every other leg here:
        // this handler is polled once per rendered row every few seconds, and a subprocess run
        // inline would occupy a runtime worker thread for its whole duration.
        let remote_repo_root = repo_root.clone();
        let remote_branch = branch.clone();
        let remote = spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "QueryBranch: remote ref",
            move || {
                Ok(
                    match remote_repo_root.as_deref().and_then(|root| {
                        tddy_core::worktree::remote_branch_ref_sha(root, &remote_branch)
                    }) {
                        Some(sha) => BranchRemote { exists: true, sha },
                        None => BranchRemote::default(),
                    },
                )
            },
        )
        .await
        .unwrap_or_else(|status| {
            // Degrades this leg alone — an absent remote ref only blocks a *descendant's* spawn, and
            // reporting the RPC as failed would take the session, worktree and PR legs down with it.
            log::warn!(
                "QueryBranch: resolving the remote ref for '{branch}' did not complete: {}",
                status.message()
            );
            BranchRemote::default()
        });

        // Base sync — how the branch stands against the base the caller named. Like every other leg
        // it never fails the call: an unnamed base, an unknown checkout, a probe that could not run
        // and a probe that timed out all arrive as `unavailable` with a reason.
        let base_sync = self
            .base_sync_leg(repo_root.as_deref(), &branch, req.base_branch.trim())
            .await;

        // PR — same path as get_pr_status. A lookup that cannot be performed degrades this leg
        // alone; the session, worktree and remote legs above stay usable.
        let pr = self
            .pr_status_for_caller(&github_user, repo_root.as_deref(), &branch)
            .await;

        Ok(Response::new(QueryBranchResponse {
            resolution: Some(BranchResolution {
                branch,
                session: Some(session),
                worktree: Some(worktree),
                pr: Some(pr),
                remote: Some(remote),
                base_sync,
            }),
        }))
    }

    async fn repoint_planned_pr(
        &self,
        request: Request<RepointPlannedPrRequest>,
    ) -> Result<Response<RepointPlannedPrResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.node_id.trim().is_empty() {
            return Err(Status::invalid_argument("node_id is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        // A repoint rebases a local branch and re-targets a PR base, so it needs the real checkout:
        // an unknown one is a precondition failure, not a repoint attempted in the session directory.
        let repo_root = tddy_core::repo_root_for_session(&session_dir).ok_or_else(|| {
            Status::failed_precondition(
                "no checkout is recorded for this session, so its repository could not be resolved",
            )
        })?;
        let owner_repo = owner_repo_from_repo_root(&repo_root).unwrap_or_default();
        let default_branch =
            tddy_core::resolve_default_integration_base_ref(&repo_root).map_err(|e| {
                Status::failed_precondition(format!("could not resolve default branch: {e}"))
            })?;

        let node_id = req.node_id.trim().to_string();

        // On the wire, an unnamed target means "the project's default branch" — the daemon's own
        // resolved ref, substituted here.
        //
        // A client cannot always name it: `ProjectEntry.main_branch_ref` is empty for a project that
        // stores no default, and the web then renders "Repoint to default branch". Forwarding that
        // empty string would instead select the recipe's drop-merged-parents rule, which in the very
        // case this feature exists for — a predecessor whose PR merged but whose plan still records
        // `open` — drops nothing at all and returns success against an unchanged plan. The operator
        // would see no error and no change.
        //
        // The recipe's `None` mode is therefore in-process only; it is never reachable from here.
        let requested_target = if req.target_base_branch.trim().is_empty() {
            default_branch.as_str()
        } else {
            req.target_base_branch.as_str()
        };

        // The named target must be a branch this node can be based onto: the repoint retains only
        // the parents that own it, so an unvalidated target is a silent plan rewrite.
        let changeset =
            tddy_core::read_changeset(&session_dir).map_err(|e| Status::internal(e.to_string()))?;
        let stack = changeset.stack.unwrap_or_default();
        let parent_branches: Vec<String> = stack
            .node(&node_id)
            .map(|node| {
                node.parents
                    .iter()
                    .filter_map(|parent_id| stack.node(parent_id)?.branch.clone())
                    .collect()
            })
            .unwrap_or_default();
        let target_base_branch = validate_repoint_target(
            requested_target,
            &default_branch,
            &parent_branches
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>(),
        )
        .map_err(Status::invalid_argument)?;

        let session_dir_for_op = session_dir.clone();
        tokio::task::spawn_blocking(move || {
            let gh = tddy_workflow_recipes::orchestrate_pr_stack::github::RealGithubPrApi::new(
                owner_repo,
            );
            tddy_workflow_recipes::pr_stack::repoint_planned_pr_node(
                &session_dir_for_op,
                &repo_root,
                &node_id,
                &default_branch,
                target_base_branch.as_deref(),
                &gh,
            )
        })
        .await
        .map_err(|e| Status::internal(format!("repoint_planned_pr join error: {e}")))?
        .map_err(Status::failed_precondition)?;

        // Re-read the just-updated changeset and reuse the same serializer as `ListSessions`
        // enrichment so the response's `stack_plan_json` is the exact wire shape `PrStackScreen`
        // already knows how to parse.
        let updated =
            tddy_core::read_changeset(&session_dir).map_err(|e| Status::internal(e.to_string()))?;
        let stack_plan_json = session_list_enrichment::stack_plan_json_for_changeset(&updated);
        Ok(Response::new(RepointPlannedPrResponse { stack_plan_json }))
    }

    /// Move one planned node up or down the operator's reading order.
    ///
    /// Touches nothing but `StackNode.display_order`: the dependency graph is a different fact, and
    /// keeping the two independent is the whole reason the field exists. Moving past either end is a
    /// successful no-op — the control at the end of the list is inert, not wrong.
    async fn reorder_planned_pr(
        &self,
        request: Request<ReorderPlannedPrRequest>,
    ) -> Result<Response<ReorderPlannedPrResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.node_id.trim().is_empty() {
            return Err(Status::invalid_argument("node_id is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        tddy_workflow_recipes::pr_stack::move_planned_pr_node(
            &session_dir,
            req.node_id.trim(),
            req.direction.trim(),
        )
        .map_err(Status::invalid_argument)?;

        // The same serializer `ListSessions` enrichment uses, so the response's `stack_plan_json` is
        // the exact wire shape `PrStackScreen`'s `parseStackPlan` already reads.
        let updated =
            tddy_core::read_changeset(&session_dir).map_err(|e| Status::internal(e.to_string()))?;
        let stack_plan_json = session_list_enrichment::stack_plan_json_for_changeset(&updated);
        Ok(Response::new(ReorderPlannedPrResponse { stack_plan_json }))
    }

    /// Take a base branch's commits into a planned node's branch, inside that node's own worktree,
    /// and push the result.
    ///
    /// A mutation, so an unknown checkout is a precondition failure rather than a degraded leg — the
    /// pull has nowhere to happen. The branch is then re-resolved **uncached**: the refs it compares
    /// have just moved, and the point of returning a resolution at all is that the row repaints
    /// without waiting for the next poll tick.
    async fn pull_base_into_branch(
        &self,
        request: Request<PullBaseIntoBranchRequest>,
    ) -> Result<Response<PullBaseIntoBranchResponse>, Status> {
        use tddy_service::proto::connection::{
            BranchRemote, BranchResolution, BranchSession, BranchWorktree,
        };

        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.node_id.trim().is_empty() {
            return Err(Status::invalid_argument("node_id is required"));
        }
        if req.base_branch.trim().is_empty() {
            return Err(Status::invalid_argument("base_branch is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        let repo_root = tddy_core::repo_root_for_session(&session_dir).ok_or_else(|| {
            Status::failed_precondition(
                "no checkout is recorded for this session, so its repository could not be resolved",
            )
        })?;

        let strategy = tddy_workflow_recipes::pr_stack::BaseSyncStrategy::from_wire(&req.strategy);
        let node_id = req.node_id.trim().to_string();
        let base_branch = req.base_branch.trim().to_string();
        let dirty_worktree_action = req.dirty_worktree_action.clone();
        let commit_message = req.commit_message.clone();
        let session_dir_for_op = session_dir.clone();
        let repo_root_for_op = repo_root.clone();
        let report = tokio::task::spawn_blocking(move || {
            tddy_workflow_recipes::pr_stack::pull_base_into_node_branch(
                &session_dir_for_op,
                &repo_root_for_op,
                &node_id,
                &base_branch,
                strategy,
                &dirty_worktree_action,
                &commit_message,
            )
        })
        .await
        .map_err(|e| Status::internal(format!("pull_base_into_branch join error: {e}")))?
        .map_err(Status::failed_precondition)?;

        // The branch the pull just moved, re-read from disk.
        let branch = tddy_core::read_changeset(&session_dir)
            .ok()
            .and_then(|changeset| changeset.stack)
            .and_then(|stack| stack.node(req.node_id.trim())?.branch.clone())
            .unwrap_or_default();

        // A sessions-directory walk, two worktree probes and a `git merge-tree` — all blocking, so
        // they go to the blocking pool together rather than tying up a runtime thread for the length
        // of four git subprocesses.
        let resolution_branch = branch.clone();
        let resolution_repo_root = repo_root.clone();
        let resolution_sessions_base = sessions_base.clone();
        let resolution_base_branch = req.base_branch.trim().to_string();
        let (session, worktree, remote, base_sync) = spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "PullBaseIntoBranch: re-read the branch",
            move || {
                let session = match crate::branch_owner::find_session_owning_branch(
                    &resolution_sessions_base,
                    &resolution_branch,
                )
                .ok()
                .flatten()
                {
                    Some(s) => BranchSession {
                        exists: true,
                        session_id: s.session_id,
                        is_active: s.is_active,
                        status: s.status,
                    },
                    None => BranchSession::default(),
                };
                let worktree = worktree_leg(Some(&resolution_repo_root), &resolution_branch);
                let remote = match tddy_core::worktree::remote_branch_ref_sha(
                    &resolution_repo_root,
                    &resolution_branch,
                ) {
                    Some(sha) => BranchRemote { exists: true, sha },
                    None => BranchRemote::default(),
                };
                // Uncached on purpose: the cache is keyed on the refs and the commits they point at,
                // and both commits may have just moved. Going through it would answer about the pair
                // the poll saw a moment ago.
                let base_sync = Some(
                    match tddy_core::base_sync::branch_base_sync(
                        &resolution_repo_root,
                        &resolution_branch,
                        &resolution_base_branch,
                    ) {
                        Ok(sync) => base_sync_view(sync),
                        Err(reason) => base_sync_unavailable(&resolution_base_branch, &reason),
                    },
                );
                Ok((session, worktree, remote, base_sync))
            },
        )
        .await
        // The pull itself already landed. Failing the whole call because re-reading the branch took
        // too long would leave the operator with no idea that it did, so the description degrades
        // and the report below still says what happened.
        .unwrap_or_else(|status| {
            let reason = format!(
                "the branch could not be re-read after the pull: {}",
                status.message()
            );
            log::warn!("PullBaseIntoBranch: {reason}");
            (
                BranchSession::default(),
                BranchWorktree::default(),
                BranchRemote::default(),
                Some(base_sync_unavailable(req.base_branch.trim(), &reason)),
            )
        });
        let pr = self
            .pr_status_for_caller(&github_user, Some(&repo_root), &branch)
            .await;

        Ok(Response::new(PullBaseIntoBranchResponse {
            resolution: Some(BranchResolution {
                branch,
                session: Some(session),
                worktree: Some(worktree),
                pr: Some(pr),
                remote: Some(remote),
                base_sync,
            }),
            strategy: report.strategy.to_string(),
            changed: report.changed,
            pushed: report.pushed,
            push_error: report.push_error.unwrap_or_default(),
        }))
    }

    /// Local peer-trust minting is not available on this transport. Peer credentials
    /// (SO_PEERCRED) exist only on the daemon's local Unix-domain socket; over ConnectRPC-HTTP or
    /// LiveKit there is no peer uid to trust, so those transports reach this tddy-rpc handler and
    /// are rejected. The UDS tonic adapter handles `MintLocalToken` itself and never delegates here.
    async fn mint_local_token(
        &self,
        _request: Request<MintLocalTokenRequest>,
    ) -> Result<Response<MintLocalTokenResponse>, Status> {
        Err(Status::unauthenticated(
            "local token minting is only available over the local socket",
        ))
    }

    type StreamLiveKitRoomsStream = MpscLiveKitRoomsStream;

    /// Stream the LiveKit server's rooms and their participants: one full snapshot, then one change
    /// event per delta found by polling the room service.
    ///
    /// Authenticates `session_token`, then spawns [`pump_rooms`], which emits the snapshot
    /// immediately and re-reads the roster on the poll cadence, diffing each read against the state
    /// **this** stream was last sent — a per-subscriber baseline, so two watchers cannot consume
    /// each other's deltas. A tick with no delta emits nothing, so an idle server yields an idle
    /// stream. The task ends when the receiver is dropped (client unsubscribe), and a roster read
    /// that fails ends the stream with that error rather than reporting an empty server.
    async fn stream_live_kit_rooms(
        &self,
        request: Request<StreamLiveKitRoomsRequest>,
    ) -> Result<Response<Self::StreamLiveKitRoomsStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let _github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<LiveKitRoomsEvent, Status>>();
        tokio::spawn(pump_rooms(
            Arc::clone(&self.room_roster),
            self.room_poll_interval,
            tx,
        ));

        Ok(Response::new(MpscLiveKitRoomsStream { rx }))
    }

    type StreamHostStatsStream = MpscHostStatsStream;

    /// Stream host telemetry for the selected daemon. Authenticates `session_token`, then spawns a
    /// sampling task that emits one `HostStatsEvent` immediately (both CPU and disk), then refreshes
    /// CPU and disk on two independent cadences, pushing an event carrying the latest CPU and disk
    /// snapshot on each tick. The task ends when the receiver is dropped (client unsubscribe).
    async fn stream_host_stats(
        &self,
        request: Request<StreamHostStatsRequest>,
    ) -> Result<Response<Self::StreamHostStatsStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let _github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<HostStatsEvent>();
        let host_stats = Arc::clone(&self.host_stats);
        let cpu_interval = self.host_cpu_interval;
        let disk_interval = self.host_disk_interval;

        tokio::spawn(async move {
            let read_cpu = |hs: &Arc<dyn HostStats>| HostCpuStats {
                per_core_percent: hs.cpu_per_core_percent(),
            };
            let read_disk = |hs: &Arc<dyn HostStats>| {
                let usage = hs.disk_for_project_dir();
                HostDiskStats {
                    available_bytes: usage.available_bytes,
                    total_bytes: usage.total_bytes,
                    project_dir: usage.project_dir,
                }
            };

            // Immediate emit: read both snapshots once so the footer populates on connect.
            let mut cpu = read_cpu(&host_stats);
            let mut disk = read_disk(&host_stats);
            if tx
                .send(HostStatsEvent {
                    cpu: Some(cpu.clone()),
                    disk: Some(disk.clone()),
                })
                .is_err()
            {
                return;
            }

            // Two independent timers: the first tick of each fires after one full period (not
            // immediately), so a tick provably reflects a fresh read of only that metric.
            let now = tokio::time::Instant::now();
            let mut cpu_tick = tokio::time::interval_at(now + cpu_interval, cpu_interval);
            let mut disk_tick = tokio::time::interval_at(now + disk_interval, disk_interval);

            loop {
                tokio::select! {
                    _ = cpu_tick.tick() => {
                        cpu = read_cpu(&host_stats);
                    }
                    _ = disk_tick.tick() => {
                        disk = read_disk(&host_stats);
                    }
                }
                if tx
                    .send(HostStatsEvent {
                        cpu: Some(cpu.clone()),
                        disk: Some(disk.clone()),
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Ok(Response::new(MpscHostStatsStream { rx }))
    }

    type StreamWorktreeStatsStream = MpscWorktreeStatsStream;

    /// Stream per-worktree disk-size status for a project. Authenticates `session_token`, resolves
    /// the project's main repo, then discovers its worktrees (with branch/diff, but **not** the
    /// expensive size walk). Emits one snapshot event carrying every worktree's current size state,
    /// then lazily enqueues size calculations (all worktrees when `recalculate_all`, otherwise only
    /// those never sized) and forwards each `Calculating` -> `Cached` transition as a single-row
    /// `updated` event. The forwarding task ends when the client drops the stream.
    async fn stream_worktree_stats(
        &self,
        request: Request<StreamWorktreeStatsRequest>,
    ) -> Result<Response<Self::StreamWorktreeStatsStream>, Status> {
        self.record_rpc_activity();
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

        // Discover worktrees + branch/diff off the async runtime, without the size walk.
        let repo = main_repo.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let diff_rows = spawn_blocking_with_timeout(
            timeout,
            "StreamWorktreeStats: git worktree list + diff",
            move || Ok(worktrees::list_worktree_diff_rows(&repo)),
        )
        .await?;

        // Branch/diff lookup keyed by path, so each later size update rebuilds a full row.
        let diff_by_path: std::collections::HashMap<PathBuf, WorktreeDiffRow> = diff_rows
            .iter()
            .map(|r| (r.path.clone(), r.clone()))
            .collect();

        let calculator = Arc::clone(&self.worktree_size_calculator);

        // Subscribe before enqueuing so no Calculating/Cached transition is missed.
        let mut updates = calculator.subscribe(project_id);

        // Snapshot: current size state per worktree (before any enqueue triggered below).
        let snapshot: Vec<WorktreeRow> = diff_rows
            .iter()
            .map(|r| {
                let state = calculator.state(project_id, &r.path);
                worktree_row_from_diff(
                    r,
                    state.status,
                    state.disk_bytes,
                    state.calculated_at_unix_ms,
                )
            })
            .collect();

        // Lazily enqueue: all worktrees on recalculate_all, otherwise only the never-sized ones.
        for r in &diff_rows {
            let status = calculator.state(project_id, &r.path).status;
            if req.recalculate_all || status == WorktreeSizeStatus::None {
                calculator.enqueue(project_id, &r.path).await;
            }
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<WorktreeStatsEvent>();
        if tx
            .send(WorktreeStatsEvent {
                snapshot,
                updated: None,
            })
            .is_err()
        {
            return Ok(Response::new(MpscWorktreeStatsStream { rx }));
        }

        tokio::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                match updates.recv().await {
                    Ok(update) => {
                        // Only forward worktrees present in the snapshot; a worktree created after
                        // this subscribe is picked up by a fresh StreamWorktreeStats call.
                        let Some(diff) = diff_by_path.get(&update.path) else {
                            continue;
                        };
                        let row = worktree_row_from_diff(
                            diff,
                            update.status,
                            update.disk_bytes,
                            update.calculated_at_unix_ms,
                        );
                        if tx
                            .send(WorktreeStatsEvent {
                                snapshot: Vec::new(),
                                updated: Some(row),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });

        Ok(Response::new(MpscWorktreeStatsStream { rx }))
    }

    /// (Re)trigger the on-disk size calculation for a single worktree. Authenticates
    /// `session_token`, resolves the project's main repo, and requires `worktree_path` to appear in
    /// `git worktree list` (membership-gated, mirroring `RemoveWorktree`), then enqueues the walk.
    /// The result surfaces on any `StreamWorktreeStats` subscriber and in `ListWorktreesForProject`.
    async fn calculate_worktree_size(
        &self,
        request: Request<CalculateWorktreeSizeRequest>,
    ) -> Result<Response<CalculateWorktreeSizeResponse>, Status> {
        self.record_rpc_activity();
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

        // Membership-gate on git's own worktree list (mirrors RemoveWorktree::NotListed -> NotFound).
        let repo_check = main_repo.clone();
        let wt_check = worktree_path.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let listed = spawn_blocking_with_timeout(
            timeout,
            "CalculateWorktreeSize: worktree membership check",
            move || Ok(worktrees::worktree_path_is_listed(&repo_check, &wt_check)),
        )
        .await?;
        if !listed {
            return Err(Status::not_found(
                "worktree path is not in git worktree list",
            ));
        }

        self.worktree_size_calculator
            .enqueue(project_id, &worktree_path)
            .await;

        Ok(Response::new(CalculateWorktreeSizeResponse {
            ok: true,
            message: String::new(),
        }))
    }

    async fn upload_session_file_chunk(
        &self,
        request: Request<UploadSessionFileChunkRequest>,
    ) -> Result<Response<UploadSessionFileChunkResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Reject an invalid session token before any filesystem access (parity with the other
        // session-dir methods).
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

        let host_path = crate::session_file_upload::write_upload_chunk(
            &sessions_base,
            &req.session_id,
            &req.upload_id,
            &req.file_name,
            &req.data,
            req.last,
        )?;

        Ok(Response::new(UploadSessionFileChunkResponse {
            host_path: host_path
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }))
    }

    async fn list_session_uploads(
        &self,
        request: Request<ListSessionUploadsRequest>,
    ) -> Result<Response<ListSessionUploadsResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        let sessions_base = self.uploads_sessions_base(&req.session_token)?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let uploads = crate::session_uploads::list_uploads(&sessions_base, &req.session_id)?;
        Ok(Response::new(ListSessionUploadsResponse {
            uploads: uploads
                .into_iter()
                .map(|u| SessionUploadEntry {
                    upload_id: u.upload_id,
                    file_name: u.file_name,
                    host_path: u.host_path.to_string_lossy().into_owned(),
                    size_bytes: u.size_bytes,
                    uploaded_at_ms: u.uploaded_at_ms,
                })
                .collect(),
        }))
    }

    async fn delete_session_upload(
        &self,
        request: Request<DeleteSessionUploadRequest>,
    ) -> Result<Response<DeleteSessionUploadResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        let sessions_base = self.uploads_sessions_base(&req.session_token)?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        crate::session_uploads::delete_upload(
            &sessions_base,
            &req.session_id,
            &req.upload_id,
            &req.file_name,
        )?;
        Ok(Response::new(DeleteSessionUploadResponse {}))
    }

    async fn upload_staged_attachment_chunk(
        &self,
        request: Request<UploadStagedAttachmentChunkRequest>,
    ) -> Result<Response<UploadStagedAttachmentChunkResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.resolve_os_user(&req.session_token)?;
        let local_id = local_instance_id_for_config(&self.config);

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "UploadStagedAttachmentChunk: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("UploadStagedAttachmentChunk")?;
            let inner =
                crate::livekit_peer_discovery::forward_upload_staged_attachment_chunk_via_livekit(
                    slot,
                    &peer_instance_id,
                    &req,
                )
                .await?;
            return Ok(Response::new(inner));
        }

        let staging_root =
            crate::session_attachment_staging::staging_root_for(&os_user, &self.staging_base_dir);
        let host_path = crate::session_attachment_staging::write_staged_chunk(
            &staging_root,
            &req.staging_id,
            &req.file_name,
            &req.data,
            req.last,
        )?;
        let entry = host_path.map(|path| {
            let meta = std::fs::metadata(&path).ok();
            StagedAttachmentEntry {
                daemon_instance_id: local_id,
                staging_id: req.staging_id,
                file_name: req.file_name,
                host_path: path.to_string_lossy().into_owned(),
                size_bytes: meta.as_ref().map(std::fs::Metadata::len).unwrap_or(0),
                staged_at_ms: file_mtime_ms(&path),
            }
        });
        Ok(Response::new(UploadStagedAttachmentChunkResponse { entry }))
    }

    async fn list_staged_attachments(
        &self,
        request: Request<ListStagedAttachmentsRequest>,
    ) -> Result<Response<ListStagedAttachmentsResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.resolve_os_user(&req.session_token)?;
        let local_id = local_instance_id_for_config(&self.config);

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "ListStagedAttachments: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("ListStagedAttachments")?;
            let inner = crate::livekit_peer_discovery::forward_list_staged_attachments_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(inner));
        }

        let staging_root =
            crate::session_attachment_staging::staging_root_for(&os_user, &self.staging_base_dir);
        let files = crate::session_attachment_staging::list_staged_attachments(
            &staging_root,
            &req.staging_id,
        )?;
        let attachments = files
            .into_iter()
            .map(|f| StagedAttachmentEntry {
                daemon_instance_id: local_id.clone(),
                staging_id: f.staging_id,
                file_name: f.file_name,
                host_path: f.host_path.to_string_lossy().into_owned(),
                size_bytes: f.size_bytes,
                staged_at_ms: f.staged_at_ms,
            })
            .collect();
        Ok(Response::new(ListStagedAttachmentsResponse { attachments }))
    }

    async fn delete_staged_attachment(
        &self,
        request: Request<DeleteStagedAttachmentRequest>,
    ) -> Result<Response<DeleteStagedAttachmentResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.resolve_os_user(&req.session_token)?;

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "DeleteStagedAttachment: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("DeleteStagedAttachment")?;
            let inner =
                crate::livekit_peer_discovery::forward_delete_staged_attachment_via_livekit(
                    slot,
                    &peer_instance_id,
                    &req,
                )
                .await?;
            return Ok(Response::new(inner));
        }

        let staging_root =
            crate::session_attachment_staging::staging_root_for(&os_user, &self.staging_base_dir);
        crate::session_attachment_staging::delete_staged_attachment(
            &staging_root,
            &req.staging_id,
            &req.file_name,
        )?;
        Ok(Response::new(DeleteStagedAttachmentResponse {}))
    }

    async fn read_host_document(
        &self,
        request: Request<ReadHostDocumentRequest>,
    ) -> Result<Response<ReadHostDocumentResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.resolve_os_user(&req.session_token)?;

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "ReadHostDocument: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("ReadHostDocument")?;
            let inner = crate::livekit_peer_discovery::forward_read_host_document_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(inner));
        }

        let scope =
            HostDocumentScope::try_from(req.scope).unwrap_or(HostDocumentScope::Unspecified);
        let doc = crate::host_documents::read_host_document_bytes(
            &os_user,
            &self.tddy_data_dir,
            &self.staging_base_dir,
            scope,
            &req.session_id,
            &req.project_id,
            &req.relative_path,
        )?;
        Ok(Response::new(ReadHostDocumentResponse {
            data: doc.data,
            byte_size: doc.byte_size,
        }))
    }

    /// Associated output stream type for [`stream_read_host_document`].
    type StreamReadHostDocumentStream = MpscResultStream<HostDocumentChunk>;

    /// Associated output stream type for [`stream_start_session`].
    type StreamStartSessionStream = MpscResultStream<StartSessionEvent>;

    async fn stream_read_host_document(
        &self,
        request: Request<ReadHostDocumentRequest>,
    ) -> Result<Response<Self::StreamReadHostDocumentStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.resolve_os_user(&req.session_token)?;

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "StreamReadHostDocument: forwarding stream to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("StreamReadHostDocument")?;
            // The owning host resolves the document under its own `os_user` mapping and applies
            // its own cap, so nothing is read locally here. A peer-side failure — or a stream that
            // stops without its terminator — arrives as an error item, terminating this stream.
            let rx = crate::livekit_peer_discovery::forward_stream_read_host_document_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(MpscResultStream { rx }));
        }

        let scope =
            HostDocumentScope::try_from(req.scope).unwrap_or(HostDocumentScope::Unspecified);
        let resolved = crate::host_documents::resolve_host_document(
            &os_user,
            &self.tddy_data_dir,
            &self.staging_base_dir,
            scope,
            &req.session_id,
            &req.project_id,
            &req.relative_path,
        )?;

        // The cap is checked before the first frame, so an over-cap document is refused rather
        // than streamed and cut short: a consumer cannot tell a truncated document from a whole
        // one once the frames have started.
        let max_bytes = self.config.max_attachment_bytes;
        if resolved.byte_size > max_bytes {
            return Err(Status::invalid_argument(format!(
                "host document exceeds this host's maximum attachment size of {max_bytes} bytes"
            )));
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<HostDocumentChunk, Status>>();
        tokio::task::spawn_blocking(move || {
            stream_document_frames(&resolved.path, resolved.byte_size, &tx);
        });
        Ok(Response::new(MpscResultStream { rx }))
    }

    async fn stream_start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<Self::StreamStartSessionStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        // Authenticate before classifying the route: forwarding opens an outbound RPC to a peer and
        // holds a pending-call slot on both hosts for the forward's whole deadline, so an
        // unauthenticated caller must never get that far. The resolved user is not used here — the
        // host that runs the session resolves it again under its own mapping — which is the same
        // order the unary `start_session` and `stream_read_host_document` already use.
        self.resolve_os_user(&req.session_token)?;

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "StreamStartSession: forwarding stream to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("StreamStartSession")?;
            // The session, its worktree and its attachments are created on the peer; only its
            // events cross back, so progress still reaches the client for the slowest case there
            // is — attachment bytes moving between two hosts.
            let rx = crate::livekit_peer_discovery::forward_stream_start_session_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(MpscResultStream { rx }));
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<StartSessionEvent, Status>>();
        // The work runs on its own task so progress reaches the client while the host is still
        // materializing, rather than all at once after the start completes. A failure terminates
        // the stream with the status — a result event is only ever sent on success.
        let service = self.clone();
        let progress_tx = tx.clone();
        tokio::spawn(async move {
            let sink = AttachmentProgressSink::streaming(progress_tx);
            let event = match service.start_session_core(req, &sink).await {
                Ok(response) => Ok(StartSessionEvent {
                    event: Some(StartSessionEventKind::Result(response.into_inner())),
                }),
                Err(status) => Err(status),
            };
            let _ = tx.send(event);
        });
        Ok(Response::new(MpscResultStream { rx }))
    }
}

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

/// Split one agent turn's answer into ordered [`HOST_DOCUMENT_FRAME_BYTES`] frames.
///
/// The stop reason rides the **final** frame, and an empty answer still yields exactly one frame, so
/// a consumer never has to tell "said nothing" from "nothing arrived" and a stream that ends without
/// a `last` frame is unambiguously a truncation. Framed rather than sent whole for the reason
/// `StreamExecuteTool` frames its results: over LiveKit anything past `MAX_CHUNK_FRAME_BYTES` is
/// chunk-framed, and one lost chunk frame wedges the call with no error at all
/// (`docs/ft/coder/rpc-multi-transport.md`).
fn agent_conversation_frames(content: &str, stop_reason: &str) -> Vec<AgentConversationChunk> {
    let mut frames: Vec<AgentConversationChunk> = Vec::new();
    let mut rest = content;
    while !rest.is_empty() {
        // Split on a char boundary at or below the budget: a frame cut mid-codepoint would not be a
        // `String` at all, and the two halves would each decode as replacement characters.
        let mut take = rest.len().min(HOST_DOCUMENT_FRAME_BYTES);
        while take > 0 && !rest.is_char_boundary(take) {
            take -= 1;
        }
        let (head, tail) = rest.split_at(take);
        frames.push(AgentConversationChunk {
            content_chunk: head.to_string(),
            ..Default::default()
        });
        rest = tail;
    }
    if frames.is_empty() {
        frames.push(AgentConversationChunk::default());
    }
    let last = frames.last_mut().expect("at least one frame");
    last.stop_reason = stop_reason.to_string();
    last.last = true;
    frames
}

/// Bytes per `StreamReadHostDocument` frame. Mirrors the 48 KiB the upload path chunks with, which
/// every transport in the stack (ConnectRPC-HTTP, LiveKit data channels) already carries per
/// message without its own chunk framing.
pub const HOST_DOCUMENT_FRAME_BYTES: usize = 48 * 1024;

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
const _: () = assert!(
    HOST_DOCUMENT_FRAME_BYTES + FRAME_ENVELOPE_HEADROOM
        <= tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES,
    "HOST_DOCUMENT_FRAME_BYTES must fit in one LiveKit data packet with envelope headroom"
);

/// Reads `path` in [`HOST_DOCUMENT_FRAME_BYTES`] slices into `tx`, stamping `total_byte_size` on
/// every frame. A zero-byte document still yields exactly one (empty) frame, so a consumer never
/// has to tell "empty document" from "stream produced nothing". A read error terminates the stream
/// with a status rather than closing it, so a partial document is never mistaken for a whole one.
fn stream_document_frames(
    path: &Path,
    total_byte_size: u64,
    tx: &tokio::sync::mpsc::UnboundedSender<Result<HostDocumentChunk, Status>>,
) {
    use std::io::Read as _;

    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            log::error!("stream_read_host_document: open {path:?} failed: {e}");
            let _ = tx.send(Err(Status::internal(format!(
                "failed to read host document: {e}"
            ))));
            return;
        }
    };

    let mut buf = vec![0u8; HOST_DOCUMENT_FRAME_BYTES];
    let mut sent_any = false;
    loop {
        let read = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                log::error!("stream_read_host_document: read {path:?} failed: {e}");
                let _ = tx.send(Err(Status::internal(format!(
                    "failed to read host document: {e}"
                ))));
                return;
            }
        };
        sent_any = true;
        if tx
            .send(Ok(HostDocumentChunk {
                data: buf[..read].to_vec(),
                total_byte_size,
            }))
            .is_err()
        {
            return;
        }
    }

    if !sent_any {
        let _ = tx.send(Ok(HostDocumentChunk {
            data: Vec::new(),
            total_byte_size,
        }));
    }
}

/// Split a worktree file's bytes into ordered [`HOST_DOCUMENT_FRAME_BYTES`] frames, stamping
/// `total_byte_size` on every one.
///
/// A zero-byte file still yields exactly **one** (empty) frame, so "the file is empty" stays
/// distinguishable from "the stream produced nothing" — AC18. The size is repeated on every frame
/// rather than sent as a header, as `HostDocumentChunk`'s is: a reader knows the total from the
/// first frame with no header frame to special-case, and a one-frame file is not a different shape
/// from a hundred-frame one.
///
/// The bytes are already in memory by the time this runs, because the reader that produced them is
/// also the thing that applies the cap: over-cap is refused before any frame exists, so nothing
/// here can be a partial file.
pub fn worktree_file_frames(bytes: &[u8]) -> Vec<WorktreeFileChunk> {
    let total_byte_size = bytes.len() as u64;
    let mut frames: Vec<WorktreeFileChunk> = bytes
        .chunks(HOST_DOCUMENT_FRAME_BYTES)
        .map(|chunk| WorktreeFileChunk {
            data: chunk.to_vec(),
            total_byte_size,
        })
        .collect();
    if frames.is_empty() {
        frames.push(WorktreeFileChunk {
            data: Vec::new(),
            total_byte_size,
        });
    }
    frames
}

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

/// Resolve the daemon's default project directory — the filesystem the Host Stats Footer reports
/// disk capacity for. Uses `$HOME` joined with the configured repos base subdirectory
/// (`repos_base_path`, default `repos`); when `$HOME` is unset the bare subdirectory is used.
///
// TODO(host-stats-footer): `DaemonConfig` currently exposes no explicit project-dir override; if
// one is added, prefer it here over the `$HOME`/`repos_base_path` derivation.
fn resolve_default_project_dir(config: &DaemonConfig) -> PathBuf {
    let repos_base = config.repos_base_path_or_default();
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(repos_base),
        None => PathBuf::from(repos_base),
    }
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

fn map_clean_worktree_error(e: CleanWorktreeError) -> Status {
    match e {
        CleanWorktreeError::NotListed => {
            Status::not_found("worktree path is not in git worktree list")
        }
        CleanWorktreeError::CannotCleanPrimary => {
            Status::failed_precondition("cannot clean primary worktree")
        }
        CleanWorktreeError::GitFailed { message } | CleanWorktreeError::Io(message) => {
            Status::internal(message)
        }
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
            .map(|m| ModelInfo {
                id: m.id,
                label: m.label,
            })
            .collect(),
        default_model: parsed.default_model,
    })
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
            Arc::new(CliSessionManager::new()),
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
            cursor_chat_id: None,
            activity_status: None,
            hook_token: None,
            sandbox: None,
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
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
mod host_stats_handler_unit_tests {
    use super::*;
    use crate::host_stats::{DiskUsage, HostStats};
    use std::sync::atomic::{AtomicU32, Ordering};
    use tddy_service::proto::connection::{HostStatsEvent, StreamHostStatsRequest};

    /// Deterministic host-stats double returning fixed per-core CPU and disk figures.
    struct FakeHostStats {
        per_core_percent: Vec<f32>,
        available_bytes: u64,
        total_bytes: u64,
        project_dir: String,
    }

    impl HostStats for FakeHostStats {
        fn cpu_per_core_percent(&self) -> Vec<f32> {
            self.per_core_percent.clone()
        }
        fn disk_for_project_dir(&self) -> DiskUsage {
            DiskUsage {
                available_bytes: self.available_bytes,
                total_bytes: self.total_bytes,
                project_dir: self.project_dir.clone(),
            }
        }
    }

    fn make_unit_config() -> crate::config::DaemonConfig {
        let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        crate::config::DaemonConfig::load(&path).unwrap()
    }

    fn make_unit_service() -> ConnectionServiceImpl {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().to_path_buf();
        let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
        let user_resolver: SessionUserResolver = Arc::new(|token| {
            if token == "valid" {
                Some("u".to_string())
            } else {
                None
            }
        });
        ConnectionServiceImpl::new(
            make_unit_config(),
            sessions_base_resolver,
            temp.path().to_path_buf(),
            user_resolver,
            None,
            None,
            None,
            Arc::new(CliSessionManager::new()),
        )
    }

    // --- StreamHostStats: single server-streaming host-info feed ---

    /// A host-stats double whose readings advance on every call, so a later stream event can be
    /// proven to reflect a *fresh* provider read (rather than a repeat of the first snapshot). The
    /// Nth CPU read returns `[N.0]`; the Nth disk read reports `available_bytes = N`.
    struct SequencedHostStats {
        cpu_reads: AtomicU32,
        disk_reads: AtomicU32,
    }

    impl SequencedHostStats {
        fn new() -> Self {
            Self {
                cpu_reads: AtomicU32::new(0),
                disk_reads: AtomicU32::new(0),
            }
        }
    }

    impl HostStats for SequencedHostStats {
        fn cpu_per_core_percent(&self) -> Vec<f32> {
            let nth = self.cpu_reads.fetch_add(1, Ordering::SeqCst) + 1;
            vec![nth as f32]
        }
        fn disk_for_project_dir(&self) -> DiskUsage {
            let nth = self.disk_reads.fetch_add(1, Ordering::SeqCst) + 1;
            DiskUsage {
                available_bytes: nth as u64,
                total_bytes: 100,
                project_dir: "/home/dev/repos".to_string(),
            }
        }
    }

    /// Await the next stream event with a bounded timeout so a missing event fails loudly instead
    /// of hanging the test.
    async fn next_event(
        stream: &mut (impl Stream<Item = Result<HostStatsEvent, Status>> + Unpin),
    ) -> HostStatsEvent {
        tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .expect("no host-stats event arrived within the timeout")
            .expect("host-stats stream closed unexpectedly")
            .expect("host-stats stream yielded an error")
    }

    #[tokio::test]
    async fn stream_host_stats_rejects_an_invalid_token() {
        // Given a service that would stream host telemetry
        let service = make_unit_service().with_host_stats(Arc::new(FakeHostStats {
            per_core_percent: vec![10.0, 55.0, 90.0, 30.0],
            available_bytes: 42_100_000_000,
            total_bytes: 100_000_000_000,
            project_dir: "/home/dev/repos".to_string(),
        }));

        // When an unauthenticated caller subscribes to the host-stats stream
        let result = service
            .stream_host_stats(Request::new(StreamHostStatsRequest {
                session_token: "bad-token".to_string(),
            }))
            .await;

        // Then the subscription is rejected as unauthenticated
        assert_eq!(result.unwrap_err().code, tddy_rpc::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn stream_host_stats_emits_cpu_and_disk_immediately_on_subscribe() {
        // Given a service reporting four cores at 10 / 55 / 90 / 30 % and 42.1 GB free of 100 GB
        let service = make_unit_service().with_host_stats(Arc::new(FakeHostStats {
            per_core_percent: vec![10.0, 55.0, 90.0, 30.0],
            available_bytes: 42_100_000_000,
            total_bytes: 100_000_000_000,
            project_dir: "/home/dev/repos".to_string(),
        }));

        // When an authenticated caller subscribes
        let mut stream = service
            .stream_host_stats(Request::new(StreamHostStatsRequest {
                session_token: "valid".to_string(),
            }))
            .await
            .unwrap()
            .into_inner();

        // Then the very first event carries both the current CPU and the current disk snapshot
        let event = next_event(&mut stream).await;
        let cpu = event.cpu.expect("first event must carry a CPU snapshot");
        let disk = event.disk.expect("first event must carry a disk snapshot");
        assert_eq!(cpu.per_core_percent, vec![10.0, 55.0, 90.0, 30.0]);
        assert_eq!(disk.available_bytes, 42_100_000_000);
        assert_eq!(disk.total_bytes, 100_000_000_000);
        assert_eq!(disk.project_dir, "/home/dev/repos");
    }

    #[tokio::test]
    async fn stream_host_stats_refreshes_cpu_on_the_fast_cadence() {
        // Given a service whose provider advances on each read, a fast CPU cadence, and a disk
        // cadence too slow to fire within the test window
        let service = make_unit_service()
            .with_host_stats(Arc::new(SequencedHostStats::new()))
            .with_host_stats_intervals(Duration::from_millis(20), Duration::from_secs(30));

        // When a caller subscribes and reads two successive events
        let mut stream = service
            .stream_host_stats(Request::new(StreamHostStatsRequest {
                session_token: "valid".to_string(),
            }))
            .await
            .unwrap()
            .into_inner();
        let first = next_event(&mut stream).await;
        let second = next_event(&mut stream).await;

        // Then each event's CPU reflects a fresh provider read (1st then 2nd), while the disk block
        // is unchanged because the slow cadence has not fired
        assert_eq!(first.cpu.expect("cpu").per_core_percent, vec![1.0]);
        assert_eq!(second.cpu.expect("cpu").per_core_percent, vec![2.0]);
        assert_eq!(
            second.disk.expect("disk").available_bytes,
            first.disk.expect("disk").available_bytes
        );
    }

    #[tokio::test]
    async fn stream_host_stats_refreshes_disk_on_the_slow_cadence() {
        // Given a service whose provider advances on each read, a fast disk cadence, and a CPU
        // cadence too slow to fire within the test window
        let service = make_unit_service()
            .with_host_stats(Arc::new(SequencedHostStats::new()))
            .with_host_stats_intervals(Duration::from_secs(30), Duration::from_millis(20));

        // When a caller subscribes and reads two successive events
        let mut stream = service
            .stream_host_stats(Request::new(StreamHostStatsRequest {
                session_token: "valid".to_string(),
            }))
            .await
            .unwrap()
            .into_inner();
        let first = next_event(&mut stream).await;
        let second = next_event(&mut stream).await;

        // Then each event's disk reflects a fresh provider read (1st then 2nd), while the CPU block
        // is unchanged because the slow cadence has not fired
        assert_eq!(first.disk.expect("disk").available_bytes, 1);
        assert_eq!(second.disk.expect("disk").available_bytes, 2);
        assert_eq!(
            second.cpu.expect("cpu").per_core_percent,
            first.cpu.expect("cpu").per_core_percent
        );
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
            Arc::new(CliSessionManager::new()),
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

    /// Daemon-direct contract (changeset 2026-07-12-fast-session-change): the web routes
    /// `DeleteSession` directly to the daemon participant (`daemon-{instanceId}`) with the
    /// caller's `session_token` — the coder is not on the path, so lifecycle control still works
    /// when the coder participant is stuck. A caller with a valid token passes auth and reaches
    /// session processing; with no such session on disk the result is a downstream error
    /// (FailedPrecondition from `delete_session_directory`), NOT `Unauthenticated`. Behaviour is
    /// unchanged from today (delete was always daemon-served); this test locks the contract.
    #[tokio::test]
    async fn delete_session_unit_accepts_daemon_direct_caller_with_valid_token() {
        let temp = tempfile::tempdir().unwrap();
        let service = make_unit_service(temp.path().to_path_buf());
        let request = Request::new(DeleteSessionRequest {
            session_token: "valid".to_string(),
            session_id: "no-such-session".to_string(),
        });
        let result = service.delete_session(request).await;
        assert!(result.is_err(), "missing session should return an error");
        let code = result.unwrap_err().code;
        assert_ne!(
            code,
            tddy_rpc::Code::Unauthenticated,
            "relay caller with a valid token must pass the auth boundary (got {code:?}); \
             the daemon must treat the coder relay's forwarded session_token identically to a \
             direct web call"
        );
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
            Arc::new(CliSessionManager::new()),
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
            cursor_chat_id: None,
            activity_status: None,
            hook_token: None,
            sandbox: None,
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
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
            Arc::new(CliSessionManager::new()),
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
            cursor_chat_id: None,
            activity_status: None,
            hook_token: Some(hook_token.to_string()),
            sandbox: None,
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
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
            cursor_chat_id: None,
            activity_status: None,
            hook_token: None,
            sandbox: None,
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
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

#[cfg(test)]
mod agent_activity_unit_tests {
    use super::*;
    use futures_util::StreamExt;
    use std::time::Duration;
    use tddy_core::agent_activity::{
        append_agent_activity, read_agent_activity, AgentActivityRecord, STATUS_COMPLETED,
        STATUS_RUNNING,
    };
    use tddy_core::session_lifecycle::unified_session_dir_path;
    use tddy_core::SessionMetadata;

    const TEST_HOOK_TOKEN: &str = "tok-activity-hook-xyz789";
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
        let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
        let user_resolver: SessionUserResolver =
            Arc::new(|token| (token == "valid").then(|| "u".to_string()));
        ConnectionServiceImpl::new(
            config,
            sessions_base_resolver,
            sessions_base,
            user_resolver,
            None,
            None,
            None,
            Arc::new(CliSessionManager::new()),
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
            project_id: "proj-activity-unit".to_string(),
            created_at: "2026-06-13T10:00:00Z".to_string(),
            updated_at: "2026-06-13T10:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/worktrees/activity-test".to_string()),
            pid: None,
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some("claude-sonnet-4-6".to_string()),
            cursor_chat_id: None,
            activity_status: None,
            hook_token: Some(hook_token.to_string()),
            sandbox: None,
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
        };
        tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
    }

    fn a_pre_tool_use(
        session_id: &str,
        tool_name: &str,
        input_json: &str,
    ) -> ReportAgentActivityRequest {
        ReportAgentActivityRequest {
            session_id: session_id.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: TEST_OS_USER.to_string(),
            event: "PreToolUse".to_string(),
            tool_name: tool_name.to_string(),
            input_json: input_json.to_string(),
            result_json: String::new(),
            is_error: false,
            error_message: String::new(),
        }
    }

    fn a_post_tool_use(
        session_id: &str,
        tool_name: &str,
        result_json: &str,
    ) -> ReportAgentActivityRequest {
        ReportAgentActivityRequest {
            session_id: session_id.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: TEST_OS_USER.to_string(),
            event: "PostToolUse".to_string(),
            tool_name: tool_name.to_string(),
            input_json: String::new(),
            result_json: result_json.to_string(),
            is_error: false,
            error_message: String::new(),
        }
    }

    fn a_seeded_record(call_id: &str, tool_name: &str) -> AgentActivityRecord {
        AgentActivityRecord {
            call_id: call_id.to_string(),
            tool_name: tool_name.to_string(),
            input: serde_json::json!({ "path": "src/main.rs" }),
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::json!({ "content": "fn main() {}" }),
            error_message: String::new(),
            started_unix_ms: 1_700_000_000_000,
            completed_unix_ms: 1_700_000_000_500,
            source: "claude-cli".to_string(),
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
        }
    }

    /// A PreToolUse then PostToolUse pair for one call appends a `running` then a terminal row that
    /// coalesce (by shared call_id) into a single completed record in `agent-activity.jsonl`.
    #[tokio::test]
    async fn report_pre_then_post_tool_use_coalesces_into_one_completed_call() {
        // Given a registered claude-cli session with a valid hook_token.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "activity-pair-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);
        let service = make_unit_service(sessions_base);

        // When the hook reports PreToolUse (Bash starts) then PostToolUse (Bash finished).
        service
            .report_agent_activity(Request::new(a_pre_tool_use(
                session_id,
                "Bash",
                r#"{"command":"cargo test"}"#,
            )))
            .await
            .unwrap();
        service
            .report_agent_activity(Request::new(a_post_tool_use(
                session_id,
                "Bash",
                r#"{"stdout":"ok","exit_code":0}"#,
            )))
            .await
            .unwrap();

        // Then the two rows coalesce into one completed call carrying the terminal state.
        let records = read_agent_activity(&session_dir).unwrap();
        assert_eq!(records.len(), 1, "the pair must coalesce into one call");
        assert_eq!(records[0].tool_name, "Bash");
        assert_eq!(records[0].status, STATUS_COMPLETED);
        assert_eq!(
            records[0].result,
            serde_json::json!({ "stdout": "ok", "exit_code": 0 })
        );
        assert_eq!(records[0].source, "claude-cli");
    }

    /// A PreToolUse alone appends a single `running` row (no terminal row yet).
    #[tokio::test]
    async fn report_pre_tool_use_alone_appends_a_running_row() {
        // Given a registered claude-cli session.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "activity-running-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);
        let service = make_unit_service(sessions_base);

        // When only a PreToolUse is reported.
        service
            .report_agent_activity(Request::new(a_pre_tool_use(
                session_id,
                "Read",
                r#"{"path":"README.md"}"#,
            )))
            .await
            .unwrap();

        // Then the single recorded call is still running.
        let records = read_agent_activity(&session_dir).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tool_name, "Read");
        assert_eq!(records[0].status, STATUS_RUNNING);
        assert_eq!(records[0].completed_unix_ms, 0);
    }

    /// A wrong hook_token is rejected before any activity is written.
    #[tokio::test]
    async fn report_agent_activity_rejects_bad_hook_token() {
        // Given a registered claude-cli session.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "activity-bad-token-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);
        let service = make_unit_service(sessions_base);

        // When the hook presents the wrong token.
        let mut req = a_pre_tool_use(session_id, "Bash", "{}");
        req.hook_token = "wrong-token".to_string();
        let err = service
            .report_agent_activity(Request::new(req))
            .await
            .unwrap_err();

        // Then it is denied and nothing is written.
        assert_eq!(err.code, tddy_rpc::Code::PermissionDenied);
        assert!(read_agent_activity(&session_dir).unwrap().is_empty());
    }

    /// StreamSessionActivity replays the persisted snapshot rows, in first-seen order, on subscribe.
    #[tokio::test]
    async fn stream_session_activity_replays_the_persisted_snapshot() {
        // Given a session with two pre-seeded agent-activity rows.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "activity-snapshot-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_core::agent_activity::append_agent_activity(
            &session_dir,
            &a_seeded_record("call-a", "Read"),
        )
        .unwrap();
        tddy_core::agent_activity::append_agent_activity(
            &session_dir,
            &a_seeded_record("call-b", "Bash"),
        )
        .unwrap();
        let service = make_unit_service(sessions_base);

        // When a client subscribes to the activity stream.
        let mut stream = service
            .stream_session_activity(Request::new(StreamSessionActivityRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::SnapshotThenLive as i32,
            }))
            .await
            .unwrap()
            .into_inner();

        // Then the two snapshot records arrive in first-seen order.
        let first = next_record(&mut stream).await;
        let second = next_record(&mut stream).await;
        assert_eq!(first.call_id, "call-a");
        assert_eq!(first.tool_name, "Read");
        assert_eq!(second.call_id, "call-b");
        assert_eq!(second.tool_name, "Bash");
    }

    /// After the snapshot, a record published to the hub for the session is delivered live.
    #[tokio::test]
    async fn stream_session_activity_delivers_a_live_record_after_the_snapshot() {
        // Given a session with one pre-seeded row and a subscribed stream.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "activity-live-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_core::agent_activity::append_agent_activity(
            &session_dir,
            &a_seeded_record("call-snapshot", "Read"),
        )
        .unwrap();
        let service = make_unit_service(sessions_base);
        let hub = service.agent_activity_hub();

        let mut stream = service
            .stream_session_activity(Request::new(StreamSessionActivityRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::SnapshotThenLive as i32,
            }))
            .await
            .unwrap()
            .into_inner();

        // Drain the snapshot record so the next awaited item is the live one.
        let snapshot = next_record(&mut stream).await;
        assert_eq!(snapshot.call_id, "call-snapshot");

        // When a fresh record is published live for this session.
        hub.publish(session_id, a_seeded_record("call-live", "Grep"));

        // Then the subscriber receives it.
        let live = next_record(&mut stream).await;
        assert_eq!(live.call_id, "call-live");
        assert_eq!(live.tool_name, "Grep");
    }

    /// In LIVE_ONLY mode the persisted snapshot is not replayed: the first record delivered is one
    /// published *after* the subscription, never a pre-seeded snapshot row.
    #[tokio::test]
    async fn stream_session_activity_in_live_only_mode_skips_the_snapshot_and_delivers_only_live_records(
    ) {
        // Given a session with a pre-seeded snapshot row.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "activity-live-only-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_core::agent_activity::append_agent_activity(
            &session_dir,
            &a_seeded_record("call-snapshot", "Read"),
        )
        .unwrap();
        let service = make_unit_service(sessions_base);
        let hub = service.agent_activity_hub();

        // When a client subscribes in LIVE_ONLY mode.
        let mut stream = service
            .stream_session_activity(Request::new(StreamSessionActivityRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: tddy_service::proto::connection::StreamMode::LiveOnly as i32,
            }))
            .await
            .unwrap()
            .into_inner();

        // and a fresh record is published live for the session.
        hub.publish(session_id, a_seeded_record("call-live", "Grep"));

        // Then the first record delivered is the live one — the snapshot was skipped entirely.
        let first = next_record(&mut stream).await;
        assert_eq!(
            first.call_id, "call-live",
            "live-only must not replay the persisted snapshot ('call-snapshot')"
        );
    }

    /// The hook sends `input_json` as a string; the persisted record carries it as structured JSON.
    #[tokio::test]
    async fn report_agent_activity_parses_the_hooks_json_input_into_a_structured_record() {
        // Given a registered claude-cli session.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "activity-parse-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);
        let service = make_unit_service(sessions_base);

        // When the hook reports a PreToolUse whose input is a JSON object string.
        service
            .report_agent_activity(Request::new(a_pre_tool_use(
                session_id,
                "Bash",
                r#"{"command":"cargo test"}"#,
            )))
            .await
            .unwrap();

        // Then the persisted record carries the parsed structured input.
        let records = read_agent_activity(&session_dir).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].input,
            serde_json::json!({ "command": "cargo test" })
        );
    }

    /// A non-JSON input string is preserved as a JSON string scalar (no data loss, no fabrication).
    #[tokio::test]
    async fn report_agent_activity_stores_a_non_json_input_string_as_a_string_scalar() {
        // Given a registered claude-cli session.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "activity-nonjson-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);
        let service = make_unit_service(sessions_base);

        // When the hook reports input that is not valid JSON.
        service
            .report_agent_activity(Request::new(a_pre_tool_use(
                session_id,
                "Bash",
                "not valid json",
            )))
            .await
            .unwrap();

        // Then it is stored as a JSON string scalar rather than dropped.
        let records = read_agent_activity(&session_dir).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].input,
            serde_json::Value::String("not valid json".to_string())
        );
    }

    /// Await the next stream item with a bounded timeout so a missing record fails loudly instead
    /// of hanging the test.
    async fn next_record(stream: &mut super::MpscAgentActivityStream) -> ProtoAgentActivityRecord {
        tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .expect("no agent-activity record arrived within the timeout")
            .expect("activity stream closed unexpectedly")
            .expect("activity stream yielded an error")
    }

    /// Await the next replay frame with a bounded timeout, decoding its inner ACP `AcpAgentMessage`.
    async fn next_replay_frame(
        stream: &mut super::MpscAcpReplayStream,
    ) -> tddy_service::proto::acp::AcpAgentMessage {
        let envelope = tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .expect("no replay frame arrived within the timeout")
            .expect("replay stream closed unexpectedly")
            .expect("replay stream yielded an error");
        prost::Message::decode(&envelope.acp_agent_message[..])
            .expect("decode inner AcpAgentMessage")
    }

    /// The text of an `agent_message_chunk` ACP frame (panics on any other shape).
    fn acp_agent_text(frame: &tddy_service::proto::acp::AcpAgentMessage) -> String {
        use tddy_service::proto::acp::{acp_agent_message, content_block, session_update};
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                match n.update.as_ref().and_then(|u| u.update.as_ref()) {
                    Some(session_update::Update::AgentMessageChunk(c)) => {
                        match c.content.as_ref().and_then(|b| b.block.as_ref()) {
                            Some(content_block::Block::Text(t)) => t.text.clone(),
                            other => panic!("expected text content, got {other:?}"),
                        }
                    }
                    other => panic!("expected AgentMessageChunk, got {other:?}"),
                }
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    /// The tool_call_id of a `tool_call` ACP frame (panics on any other shape).
    fn acp_tool_call_id(frame: &tddy_service::proto::acp::AcpAgentMessage) -> String {
        use tddy_service::proto::acp::{acp_agent_message, session_update};
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                match n.update.as_ref().and_then(|u| u.update.clone()) {
                    Some(session_update::Update::ToolCall(tc)) => {
                        tc.tool_call_id.map(|id| id.value).unwrap_or_default()
                    }
                    other => panic!("expected ToolCall, got {other:?}"),
                }
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    /// StreamAcpReplay replays the persisted transcript snapshot, in write order, on subscribe.
    #[tokio::test]
    async fn stream_acp_replay_replays_the_persisted_snapshot() {
        // Given a session with a pre-seeded ACP transcript (an agent-text then a tool_call frame).
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-snapshot-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::agent_text_frame("Analyzing the parser.", 1_000),
        )
        .unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
        )
        .unwrap();
        let service = make_unit_service(sessions_base);

        // When a client subscribes to the ACP replay stream.
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::SnapshotThenLive as i32,
                page_size: 0,
            }))
            .await
            .unwrap()
            .into_inner();

        // Then the two snapshot frames arrive in write order.
        let first = next_replay_frame(&mut stream).await;
        let second = next_replay_frame(&mut stream).await;
        assert_eq!(acp_agent_text(&first), "Analyzing the parser.");
        assert_eq!(acp_tool_call_id(&second), "call-a");
    }

    /// After the snapshot, a record published to the hub is delivered live as an ACP tool_call frame.
    #[tokio::test]
    async fn stream_acp_replay_delivers_a_live_frame_after_the_snapshot() {
        // Given a session with one pre-seeded transcript frame and a subscribed stream.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-live-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::agent_text_frame("Starting.", 1_000),
        )
        .unwrap();
        let service = make_unit_service(sessions_base);
        let hub = service.agent_activity_hub();

        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::SnapshotThenLive as i32,
                page_size: 0,
            }))
            .await
            .unwrap()
            .into_inner();

        // Drain the snapshot frame so the next awaited item is the live one.
        let snapshot = next_replay_frame(&mut stream).await;
        assert_eq!(acp_agent_text(&snapshot), "Starting.");

        // When a fresh record is published live for this session.
        hub.publish(session_id, a_seeded_record("call-live", "Grep"));

        // Then the subscriber receives it as an ACP tool_call frame.
        let live = next_replay_frame(&mut stream).await;
        assert_eq!(acp_tool_call_id(&live), "call-live");
    }

    /// Pull one raw `AcpReplayFrame` envelope (the count-carrying wrapper), with a timeout so a
    /// count-mode subscription that never broadcasts a count fails fast instead of hanging.
    async fn next_replay_envelope(
        stream: &mut super::MpscAcpReplayStream,
    ) -> tddy_service::proto::connection::AcpReplayFrame {
        tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .expect("no replay frame arrived within the timeout")
            .expect("replay stream closed unexpectedly")
            .expect("replay stream yielded an error")
    }

    /// COUNT_THEN_LIVE emits the current persisted-frame count first (no transcript payload), then a
    /// fresh count each time a new activity is published — the cheap feed that drives the overlay's
    /// icon/badge before the pane is opened.
    #[tokio::test]
    async fn stream_acp_replay_count_then_live_broadcasts_the_activity_count() {
        // Given a session whose persisted transcript already holds three frames.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-count-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::agent_text_frame("Analyzing.", 1_000),
        )
        .unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
        )
        .unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-b", "Grep")),
        )
        .unwrap();
        let service = make_unit_service(sessions_base);
        let hub = service.agent_activity_hub();

        // When a client subscribes in count-first mode.
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::CountThenLive as i32,
                page_size: 0,
            }))
            .await
            .unwrap()
            .into_inner();

        // Then the first frame carries the current count (3) and no transcript payload.
        let first = next_replay_envelope(&mut stream).await;
        assert_eq!(first.activity_count, 3);
        assert!(
            first.acp_agent_message.is_empty(),
            "a count frame must not carry a transcript payload"
        );

        // And a newly-published activity raises the broadcast count to 4.
        hub.publish(session_id, a_seeded_record("call-live", "Bash"));
        let next = next_replay_envelope(&mut stream).await;
        assert_eq!(next.activity_count, 4);
    }

    /// A single tool call publishes two records (running then terminal) under one call_id; the count
    /// must rise by one, not two — matching the single coalesced row the pane renders.
    #[tokio::test]
    async fn stream_acp_replay_count_then_live_counts_a_tool_call_once_across_its_two_records() {
        // Given a session whose transcript holds one agent-text frame (count baseline 1).
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-count-dedupe-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::agent_text_frame("Analyzing.", 1_000),
        )
        .unwrap();
        let service = make_unit_service(sessions_base);
        let hub = service.agent_activity_hub();

        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::CountThenLive as i32,
                page_size: 0,
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(next_replay_envelope(&mut stream).await.activity_count, 1);

        // When a tool call publishes its running then terminal record under the same call_id.
        hub.publish(session_id, a_seeded_record("call-x", "Bash"));
        // Then the first (running) record lifts the count to 2.
        assert_eq!(next_replay_envelope(&mut stream).await.activity_count, 2);
        // The terminal record for call-x emits nothing; a distinct call is the next frame — and it
        // reads 3, proving call-x was not counted twice.
        hub.publish(session_id, a_seeded_record("call-x", "Bash"));
        hub.publish(session_id, a_seeded_record("call-y", "Read"));
        assert_eq!(next_replay_envelope(&mut stream).await.activity_count, 3);
    }

    // -----------------------------------------------------------------------
    // Persisted-activity replay (bug fc990524: badge counts, pane opens empty)
    //
    // `acp-transcript.jsonl` is written by the tddy-coder presenter seam only. Every daemon-hosted
    // (claude-cli / sandbox) session on disk therefore has a large `agent-activity.jsonl` and NO
    // `acp-transcript.jsonl` — so an ACP replay that reads the transcript file alone serves an empty
    // snapshot while its count feed keeps counting live records: the operator sees a badge, opens
    // the pane, and finds nothing (and nothing at all after a page reload).
    // -----------------------------------------------------------------------

    /// The snapshot must project the session's durable `agent-activity.jsonl` rows, which are the
    /// only persisted record of a daemon-hosted session's tool calls.
    #[tokio::test]
    async fn stream_acp_replay_replays_persisted_agent_activity_when_no_acp_transcript_exists() {
        // Given a session whose durable activity log holds two completed calls and which has no
        // `acp-transcript.jsonl` at all (the on-disk shape of every claude-cli session).
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-legacy-activity-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        append_agent_activity(&session_dir, &a_seeded_record("call-a", "Read")).unwrap();
        append_agent_activity(&session_dir, &a_seeded_record("call-b", "Grep")).unwrap();
        let service = make_unit_service(sessions_base);

        // When a client subscribes to the ACP replay snapshot.
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::SnapshotThenLive as i32,
                page_size: 0,
            }))
            .await
            .unwrap()
            .into_inner();

        // Then both persisted calls are replayed as tool_call frames, in recorded order.
        assert_eq!(
            acp_tool_call_id(&next_replay_frame(&mut stream).await),
            "call-a"
        );
        assert_eq!(
            acp_tool_call_id(&next_replay_frame(&mut stream).await),
            "call-b"
        );
    }

    /// The badge must never promise entries the pane cannot deliver: the count baseline is taken from
    /// the same resolved transcript the snapshot replays, so persisted activity counts even when no
    /// `acp-transcript.jsonl` was ever written.
    #[tokio::test]
    async fn stream_acp_replay_count_then_live_counts_persisted_agent_activity_rows() {
        // Given a session whose durable activity log holds two completed calls and no ACP transcript.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-legacy-count-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        append_agent_activity(&session_dir, &a_seeded_record("call-a", "Read")).unwrap();
        append_agent_activity(&session_dir, &a_seeded_record("call-b", "Grep")).unwrap();
        let service = make_unit_service(sessions_base);

        // When a client subscribes in count-first mode.
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::CountThenLive as i32,
                page_size: 0,
            }))
            .await
            .unwrap()
            .into_inner();

        // Then the first count frame reports the two persisted calls.
        assert_eq!(next_replay_envelope(&mut stream).await.activity_count, 2);
    }

    /// The full `ToolCall` payload of a `tool_call` ACP frame (panics on any other shape).
    fn acp_tool_call(
        frame: &tddy_service::proto::acp::AcpAgentMessage,
    ) -> tddy_service::proto::acp::ToolCall {
        use tddy_service::proto::acp::{acp_agent_message, session_update};
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                match n.update.as_ref().and_then(|u| u.update.clone()) {
                    Some(session_update::Update::ToolCall(tc)) => tc,
                    other => panic!("expected ToolCall, got {other:?}"),
                }
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    /// A `SNAPSHOT_THEN_LIVE` tool-call frame carries the call's metadata but not its bodies: the
    /// heavy `raw_input`/`raw_output` are stripped so the stream's size tracks the number of tool
    /// calls, not the volume of their I/O.
    #[tokio::test]
    async fn stream_acp_replay_snapshot_frames_omit_tool_bodies() {
        // Given a session whose persisted transcript holds a completed Read call with a full
        // raw_input and raw_output baked into the frame.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-strip-snapshot-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
        )
        .unwrap();
        let service = make_unit_service(sessions_base);

        // When a client subscribes to the snapshot replay.
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::SnapshotThenLive as i32,
                page_size: 0,
            }))
            .await
            .unwrap()
            .into_inner();

        // Then the tool call arrives with its id and title intact but its bodies stripped.
        let tc = acp_tool_call(&next_replay_frame(&mut stream).await);
        assert_eq!(tc.tool_call_id.expect("tool_call_id").value, "call-a");
        assert_eq!(tc.title, "Read");
        assert_eq!(tc.raw_input, None);
        assert_eq!(tc.raw_output, None);
    }

    /// The live tail is stripped too: a record published after subscribe (LIVE_ONLY) arrives as a
    /// body-less tool-call frame, same as the snapshot.
    #[tokio::test]
    async fn stream_acp_replay_live_frames_omit_tool_bodies() {
        // Given a live subscription in LIVE_ONLY mode (no snapshot).
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-strip-live-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let service = make_unit_service(sessions_base);
        let hub = service.agent_activity_hub();
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::LiveOnly as i32,
                page_size: 0,
            }))
            .await
            .unwrap()
            .into_inner();

        // When a completed call is published live for this session.
        hub.publish(session_id, a_seeded_record("call-live", "Grep"));

        // Then the live tool-call frame carries its id but neither body.
        let tc = acp_tool_call(&next_replay_frame(&mut stream).await);
        assert_eq!(tc.tool_call_id.expect("tool_call_id").value, "call-live");
        assert_eq!(tc.raw_input, None);
        assert_eq!(tc.raw_output, None);
    }

    /// The bodies the stream strips are fetched on demand: GetAcpToolCallDetail returns the exact
    /// raw_input/raw_output the transcript recorded for one call.
    #[tokio::test]
    async fn get_acp_tool_call_detail_returns_the_full_tool_bodies() {
        // Given a session whose transcript holds a completed Read call.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-detail-full-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
        )
        .unwrap();
        let service = make_unit_service(sessions_base);

        // When the detail for that call is requested.
        let detail = service
            .get_acp_tool_call_detail(Request::new(GetAcpToolCallDetailRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                tool_call_id: "call-a".to_string(),
            }))
            .await
            .expect("detail lookup should succeed")
            .into_inner();

        // Then it returns the exact bodies the stream used to inline.
        let raw_input: serde_json::Value =
            serde_json::from_str(&detail.raw_input.expect("raw_input")).expect("raw_input is JSON");
        let raw_output: serde_json::Value =
            serde_json::from_str(&detail.raw_output.expect("raw_output"))
                .expect("raw_output is JSON");
        assert_eq!(raw_input, serde_json::json!({ "path": "src/main.rs" }));
        assert_eq!(raw_output, serde_json::json!({ "content": "fn main() {}" }));
    }

    /// A tool_call_id absent from the transcript is a NOT_FOUND error, not an empty success — so the
    /// caller can tell "no such call" from "call exists but has no output".
    #[tokio::test]
    async fn get_acp_tool_call_detail_is_not_found_for_an_unknown_tool_call_id() {
        // Given a session whose transcript holds only call-a.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-detail-missing-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
        )
        .unwrap();
        let service = make_unit_service(sessions_base);

        // When the detail for a non-existent call is requested.
        let status = service
            .get_acp_tool_call_detail(Request::new(GetAcpToolCallDetailRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                tool_call_id: "does-not-exist".to_string(),
            }))
            .await
            .expect_err("an unknown tool_call_id should be an error");

        // Then the status is NOT_FOUND.
        assert_eq!(status.code(), tddy_rpc::Code::NotFound);
    }

    // -----------------------------------------------------------------------
    // Tail-first replay and the reverse cursor
    // -----------------------------------------------------------------------

    use tddy_service::proto::connection::GetAcpReplayPageRequest;

    /// Seed a session dir with `entry_count` agent-text frames labelled `Entry 1` … `Entry N`
    /// (1-based, naming the entry's position in the whole transcript) and return its dir.
    fn a_session_recording(sessions_base: &std::path::Path, session_id: &str, entry_count: usize) {
        let session_dir = unified_session_dir_path(sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        for n in 1..=entry_count {
            tddy_service::acp_replay::append_acp_frame(
                &session_dir,
                &tddy_service::acp_replay::agent_text_frame(
                    &format!("Entry {n}"),
                    1_000 * n as i64,
                ),
            )
            .unwrap();
        }
    }

    /// Tail mode replays the newest page only — a long transcript no longer costs its whole history
    /// to render a screenful of its end.
    #[tokio::test]
    async fn stream_acp_replay_in_tail_mode_replays_only_the_newest_page() {
        // Given a session whose persisted transcript holds five entries.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-tail-1";
        a_session_recording(&sessions_base, session_id, 5);
        let service = make_unit_service(sessions_base);

        // When a client subscribes tail-first for a page of two.
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::TailThenLive as i32,
                page_size: 2,
            }))
            .await
            .unwrap()
            .into_inner();

        // Then only the newest two arrive, oldest-first *within* the page — entries 1-3 are not
        // replayed at all, and are reached by paging backwards instead.
        let first = next_replay_frame(&mut stream).await;
        let second = next_replay_frame(&mut stream).await;
        assert_eq!(
            (acp_agent_text(&first), acp_agent_text(&second)),
            ("Entry 4".to_string(), "Entry 5".to_string())
        );
    }

    /// Every tail frame carries its position in the **whole** transcript, not its index within the
    /// page — the first frame's `seq` is what the client pages backwards from.
    #[tokio::test]
    async fn stream_acp_replay_tail_frames_carry_their_absolute_transcript_position() {
        // Given a session whose persisted transcript holds five entries.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-tail-seq-1";
        a_session_recording(&sessions_base, session_id, 5);
        let service = make_unit_service(sessions_base);

        // When the newest page of two is replayed.
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::TailThenLive as i32,
                page_size: 2,
            }))
            .await
            .unwrap()
            .into_inner();

        // Then the page's frames are stamped 3 and 4 — their 0-based positions in the transcript.
        let first = next_replay_envelope(&mut stream).await;
        let second = next_replay_envelope(&mut stream).await;
        assert_eq!((first.seq, second.seq), (3, 4));
    }

    /// The reverse cursor: one page of frames strictly older than `before_seq`, oldest-first.
    #[tokio::test]
    async fn get_acp_replay_page_serves_the_frames_before_the_cursor() {
        // Given a session whose persisted transcript holds five entries.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-page-1";
        a_session_recording(&sessions_base, session_id, 5);
        let service = make_unit_service(sessions_base);

        // When the page before seq 3 is requested, two frames wide.
        let page = service
            .get_acp_replay_page(Request::new(GetAcpReplayPageRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                before_seq: 3,
                page_size: 2,
            }))
            .await
            .expect("page lookup should succeed")
            .into_inner();

        // Then entries at seq 1 and 2 come back, oldest-first, with the head still further back.
        let texts: Vec<String> = page
            .frames
            .iter()
            .map(|bytes| {
                acp_agent_text(
                    &prost::Message::decode(&bytes[..]).expect("decode paged AcpAgentMessage"),
                )
            })
            .collect();
        assert_eq!((page.first_seq, page.at_oldest), (1, false));
        assert_eq!(texts, vec!["Entry 2".to_string(), "Entry 3".to_string()]);
    }

    /// A paged frame is not a back door to the bodies: `GetAcpReplayPage` applies the same
    /// `strip_tool_body` seam the replay stream does.
    #[tokio::test]
    async fn get_acp_replay_page_strips_tool_bodies_like_the_replay_stream_does() {
        // Given a session whose transcript holds a completed Read call with full bodies, followed
        // by one entry, so the call sits strictly before the cursor.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-page-strip-1";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
        )
        .unwrap();
        let service = make_unit_service(sessions_base);

        // When that call is paged back rather than streamed.
        let page = service
            .get_acp_replay_page(Request::new(GetAcpReplayPageRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                before_seq: 1,
                page_size: 10,
            }))
            .await
            .expect("page lookup should succeed")
            .into_inner();

        // Then it arrives with its id and title intact but its bodies stripped.
        let frame: tddy_service::proto::acp::AcpAgentMessage =
            prost::Message::decode(&page.frames[0][..]).expect("decode paged AcpAgentMessage");
        let tc = acp_tool_call(&frame);
        assert_eq!(tc.tool_call_id.expect("tool_call_id").value, "call-a");
        assert_eq!(
            (tc.title, tc.raw_input, tc.raw_output),
            ("Read".to_string(), None, None)
        );
    }

    /// The `running` record a tool call publishes when it starts: the same call as
    /// [`a_seeded_record`], before its result is known.
    fn a_running_record(call_id: &str, tool_name: &str) -> AgentActivityRecord {
        AgentActivityRecord {
            status: STATUS_RUNNING.to_string(),
            result: serde_json::Value::Null,
            completed_unix_ms: 0,
            ..a_seeded_record(call_id, tool_name)
        }
    }

    /// A tool call's terminal record refines the entry its `running` record created, so it carries
    /// that entry's position — the two records coalesce into one transcript row, and a live reader
    /// must be able to replace the row it already placed rather than append a second one.
    #[tokio::test]
    async fn stream_acp_replay_gives_a_tool_calls_terminal_record_the_position_of_its_running_record(
    ) {
        // Given a session whose transcript holds two entries, streamed tail-first.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-live-refine-1";
        a_session_recording(&sessions_base, session_id, 2);
        let service = make_unit_service(sessions_base);
        let hub = service.agent_activity_hub();
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::TailThenLive as i32,
                page_size: 2,
            }))
            .await
            .unwrap()
            .into_inner();
        next_replay_envelope(&mut stream).await;
        next_replay_envelope(&mut stream).await;

        // When a tool call publishes its running record and then its terminal one.
        hub.publish(session_id, a_running_record("call-x", "Bash"));
        hub.publish(session_id, a_seeded_record("call-x", "Bash"));

        // Then both land on position 2 — the position a re-read of the transcript would give the
        // single row they coalesce into.
        let running = next_replay_envelope(&mut stream).await;
        let terminal = next_replay_envelope(&mut stream).await;
        assert_eq!((running.seq, terminal.seq), (2, 2));
    }

    /// A refinement must not cost the transcript a position: the next distinct call takes the very
    /// next one, so live numbering neither drifts ahead of nor lags behind a later re-read.
    #[tokio::test]
    async fn stream_acp_replay_gives_the_call_after_a_refinement_the_next_position() {
        // Given a session whose transcript holds two entries, streamed tail-first.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-live-refine-2";
        a_session_recording(&sessions_base, session_id, 2);
        let service = make_unit_service(sessions_base);
        let hub = service.agent_activity_hub();
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::TailThenLive as i32,
                page_size: 2,
            }))
            .await
            .unwrap()
            .into_inner();
        next_replay_envelope(&mut stream).await;
        next_replay_envelope(&mut stream).await;

        // When one call runs to completion and a second, distinct call then starts.
        hub.publish(session_id, a_running_record("call-x", "Bash"));
        hub.publish(session_id, a_seeded_record("call-x", "Bash"));
        hub.publish(session_id, a_running_record("call-y", "Read"));

        // Then the new call is at 3: the refinement of call-x consumed no position of its own.
        next_replay_envelope(&mut stream).await;
        next_replay_envelope(&mut stream).await;
        let new_call = next_replay_envelope(&mut stream).await;
        assert_eq!(new_call.seq, 3);
    }

    /// `LIVE_ONLY` replays none of the transcript but still numbers against it: a live frame's
    /// position is where that entry sits in the whole recording, not where it sits in the handful of
    /// frames this subscription happened to see. Reading the transcript purely to establish that base
    /// is the only reason the mode touches the disk at all, so this is what pays for it.
    #[tokio::test]
    async fn stream_acp_replay_live_only_frames_are_numbered_from_the_recorded_transcript() {
        // Given a session with three recorded entries, subscribed live-only (nothing replayed).
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-live-only-seq-1";
        a_session_recording(&sessions_base, session_id, 3);
        let service = make_unit_service(sessions_base);
        let hub = service.agent_activity_hub();
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::LiveOnly as i32,
                page_size: 0,
            }))
            .await
            .unwrap()
            .into_inner();

        // When a new call is published live.
        hub.publish(session_id, a_seeded_record("call-live", "Grep"));

        // Then it is at position 3 — after the three recorded entries — rather than claiming
        // position 0 as it would if the mode numbered from its own first delivery.
        assert_eq!(next_replay_envelope(&mut stream).await.seq, 3);
    }

    /// A call that straddles the subscribe boundary — its `running` record already in the snapshot,
    /// its terminal record arriving live — refines the snapshot's row, so it carries the position
    /// the snapshot gave that row rather than a fresh one at the tail.
    #[tokio::test]
    async fn stream_acp_replay_gives_a_live_terminal_record_the_position_its_call_holds_in_the_snapshot(
    ) {
        // Given a session whose snapshot is one agent-text entry followed by a still-running call.
        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().to_path_buf();
        let session_id = "acp-replay-live-refine-3";
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::agent_text_frame("Reading the parser.", 1_000),
        )
        .unwrap();
        append_agent_activity(&session_dir, &a_running_record("call-a", "Read")).unwrap();
        let service = make_unit_service(sessions_base);
        let hub = service.agent_activity_hub();
        let mut stream = service
            .stream_acp_replay(Request::new(StreamAcpReplayRequest {
                session_token: "valid".to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::SnapshotThenLive as i32,
                page_size: 0,
            }))
            .await
            .unwrap()
            .into_inner();
        next_replay_envelope(&mut stream).await;
        let snapshot_call = next_replay_envelope(&mut stream).await;
        assert_eq!(snapshot_call.seq, 1);

        // When that same call reports its terminal record live.
        hub.publish(session_id, a_seeded_record("call-a", "Read"));

        // Then it arrives at position 1 — replacing the snapshot's row, not appended after it.
        let terminal = next_replay_envelope(&mut stream).await;
        assert_eq!(terminal.seq, 1);
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
        serde_yaml::from_str(yaml)
            .expect("test metadata YAML must deserialize into SessionMetadata")
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
        assert!(
            agent.is_none(),
            "legacy session has no persisted agent to restore"
        );
        assert!(
            recipe.is_none(),
            "legacy session has no persisted recipe to restore"
        );
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
            Arc::new(CliSessionManager::new()),
        )
    }

    /// A def under `<tddyhome>/agents`, which is where every YAML-defined agent comes from.
    fn an_agent_def(tddy_home: &std::path::Path, name: &str) {
        let agents = tddy_home.join("agents");
        std::fs::create_dir_all(&agents).expect("create agents dir");
        std::fs::write(
            agents.join(format!("{name}.yaml")),
            format!("name: {name}\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n"),
        )
        .expect("write agent def");
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
    #[tokio::test]
    async fn resolve_specialized_agent_defs_with_no_names_produces_no_defs() {
        // Given
        let tddy_home = tempfile::tempdir().unwrap();
        let service = make_unit_service(tddy_home.path().to_path_buf());

        // When
        let result = service.resolve_specialized_agent_defs(&[]).await;

        // Then
        assert_eq!(
            result.unwrap(),
            Vec::<tddy_discovery::agent_def::SpecializedAgentDef>::new(),
            "an empty specialized_agents list must resolve to no defs, not an error"
        );
    }

    /// A `<tddyhome>/agents` def resolves by name. That directory is the only place a YAML-defined
    /// agent can come from — nothing resolves out of the binary.
    #[tokio::test]
    async fn resolve_specialized_agent_defs_resolves_a_def_from_the_agents_dir() {
        // Given
        let tddy_home = tempfile::tempdir().unwrap();
        an_agent_def(tddy_home.path(), "explorer");
        let service = make_unit_service(tddy_home.path().to_path_buf());

        // When
        let result = service
            .resolve_specialized_agent_defs(&["explorer".to_string()])
            .await;

        // Then
        let defs = result.expect("a def under <tddyhome>/agents must resolve by name");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "explorer");
    }

    /// A name that resolves against no def source must reject the whole request — no partial
    /// resolution for the names that *did* resolve.
    #[tokio::test]
    async fn resolve_specialized_agent_defs_rejects_unknown_name() {
        // Given
        let tddy_home = tempfile::tempdir().unwrap();
        an_agent_def(tddy_home.path(), "explorer");
        let service = make_unit_service(tddy_home.path().to_path_buf());

        // When
        let result = service
            .resolve_specialized_agent_defs(&["explorer".to_string(), "ghost-agent".to_string()])
            .await;

        // Then
        let err = result.expect_err("an unresolvable name must reject the whole request");
        assert_eq!(err.code(), tddy_rpc::Code::InvalidArgument);
        assert!(
            err.message().contains("ghost-agent"),
            "the error must name the unresolvable subagent; got: {}",
            err.message()
        );
    }

    /// A resolved def produces both `TDDY_SUBAGENT` (comma names) and `TDDY_SUBAGENTS_JSON` (the
    /// serialized def) — the exact env shape `tddy-tools --mcp` (see `subagents_from_env` in
    /// `tddy-tools/src/server.rs`) expects.
    #[tokio::test]
    async fn specialized_subagent_env_builds_env_pairs_for_a_resolved_def() {
        // Given
        let tddy_home = tempfile::tempdir().unwrap();
        an_agent_def(tddy_home.path(), "explorer");
        let service = make_unit_service(tddy_home.path().to_path_buf());
        let defs = service
            .resolve_specialized_agent_defs(&["explorer".to_string()])
            .await
            .expect("a def under <tddyhome>/agents must resolve by name");

        // When
        let result = service.specialized_subagent_env(&defs);

        // Then
        let env = result.expect("a resolved def must build env pairs without error");
        let names = env
            .iter()
            .find(|(k, _)| k == "TDDY_SUBAGENT")
            .map(|(_, v)| v.clone());
        assert_eq!(names.as_deref(), Some("explorer"));
        let defs_json = env
            .iter()
            .find(|(k, _)| k == "TDDY_SUBAGENTS_JSON")
            .map(|(_, v)| v.clone())
            .expect("TDDY_SUBAGENTS_JSON must be present");
        assert!(
            defs_json.contains("explorer"),
            "TDDY_SUBAGENTS_JSON must serialize the resolved def; got: {defs_json}"
        );
    }
}

#[cfg(test)]
mod add_planned_pr_unit_tests {
    //! Unit tests: `ConnectionServiceImpl::add_planned_pr` — the recipe guard rejecting a
    //! non-"pr-stack" session before its `Changeset.stack` is touched.
    //!
    //! PRD: docs/ft/coder/pr-stacking.md § Manually adding a planned PR.
    //! Changeset: docs/dev/1-WIP/pr-stack-manual-add-planned-pr.md.

    use super::*;
    use tddy_core::changeset::{read_changeset, write_changeset};

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
            sessions_base,
            user_resolver,
            None,
            None,
            None,
            Arc::new(CliSessionManager::new()),
        )
    }

    fn write_unit_changeset(session_dir: &std::path::Path, recipe: Option<&str>) {
        std::fs::create_dir_all(session_dir).unwrap();
        let changeset = Changeset {
            recipe: recipe.map(str::to_string),
            ..Changeset::default()
        };
        write_changeset(session_dir, &changeset).unwrap();
    }

    fn a_request(session_id: &str, title: &str) -> Request<AddPlannedPrRequest> {
        Request::new(AddPlannedPrRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            title: title.to_string(),
            description: String::new(),
            branch_suggestion: String::new(),
            parents: vec![],
            child_recipe: String::new(),
        })
    }

    /// A session whose changeset `recipe` is `"tdd"` (not a pr-stack orchestrator) must be
    /// rejected before `Changeset.stack` is ever touched.
    #[tokio::test]
    async fn add_planned_pr_rejects_a_session_whose_recipe_is_not_pr_stack() {
        // Given — a plain "tdd" session, not a pr-stack orchestrator
        let temp = tempfile::tempdir().unwrap();
        let service = make_unit_service(temp.path().to_path_buf());
        let session_dir = unified_session_dir_path(temp.path(), "tdd-session-1");
        write_unit_changeset(&session_dir, Some("tdd"));

        // When
        let result = service
            .add_planned_pr(a_request("tdd-session-1", "Add auth middleware"))
            .await;

        // Then
        let err = result.expect_err("a non-pr-stack session must be rejected");
        assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
        let loaded = read_changeset(&session_dir).unwrap();
        assert!(
            loaded.stack.is_none(),
            "the rejected session's Changeset.stack must remain untouched"
        );
    }

    /// A session with no `recipe` set at all (legacy/never-tagged changeset) is not a pr-stack
    /// orchestrator either, and must be rejected the same way.
    #[tokio::test]
    async fn add_planned_pr_rejects_a_session_with_no_recipe_set() {
        // Given
        let temp = tempfile::tempdir().unwrap();
        let service = make_unit_service(temp.path().to_path_buf());
        let session_dir = unified_session_dir_path(temp.path(), "no-recipe-session");
        write_unit_changeset(&session_dir, None);

        // When
        let result = service
            .add_planned_pr(a_request("no-recipe-session", "Add auth middleware"))
            .await;

        // Then
        let err = result.expect_err("a session with no recipe must be rejected");
        assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
    }

    /// A genuine "pr-stack" orchestrator session is accepted and gains the new planned PR.
    #[tokio::test]
    async fn add_planned_pr_succeeds_for_a_pr_stack_orchestrator_session() {
        // Given
        let temp = tempfile::tempdir().unwrap();
        let service = make_unit_service(temp.path().to_path_buf());
        let session_dir = unified_session_dir_path(temp.path(), "pr-stack-session-1");
        write_unit_changeset(&session_dir, Some("pr-stack"));

        // When
        let result = service
            .add_planned_pr(a_request("pr-stack-session-1", "Add auth middleware"))
            .await;

        // Then
        let resp = result
            .expect("a pr-stack orchestrator session must be accepted")
            .into_inner();
        let parsed: serde_json::Value = serde_json::from_str(&resp.stack_plan_json)
            .expect("stack_plan_json must be valid JSON");
        assert_eq!(parsed["nodes"][0]["node_id"], "n1");
        assert_eq!(parsed["nodes"][0]["title"], "Add auth middleware");
        assert_eq!(parsed["nodes"][0]["parents"], serde_json::json!([]));
        let loaded = read_changeset(&session_dir).unwrap();
        assert_eq!(loaded.stack.unwrap().nodes.len(), 1);
    }

    /// A legacy alias recipe name ("orchestrate-pr-stack") resolves to the same canonical
    /// "pr-stack" recipe and must also be accepted — this guard must not regress old sessions.
    #[tokio::test]
    async fn add_planned_pr_succeeds_for_a_legacy_orchestrate_pr_stack_alias_session() {
        // Given
        let temp = tempfile::tempdir().unwrap();
        let service = make_unit_service(temp.path().to_path_buf());
        let session_dir = unified_session_dir_path(temp.path(), "legacy-orchestrator-session");
        write_unit_changeset(&session_dir, Some("orchestrate-pr-stack"));

        // When
        let result = service
            .add_planned_pr(a_request(
                "legacy-orchestrator-session",
                "Add auth middleware",
            ))
            .await;

        // Then
        assert!(
            result.is_ok(),
            "a legacy orchestrate-pr-stack alias session must still be accepted"
        );
    }
}

/// A spawned child must record its **branch** on the planned node it materializes. Without that
/// forward link the orchestrator's stack still reads "no branch anywhere", so `base_ref_for_spawn`
/// refuses every descendant — a stack wedged at its bottom node. The child session id is recorded
/// alongside it only as a fallback route back to the branch.
#[cfg(test)]
mod stack_child_link_tests {
    use super::*;
    use tddy_core::changeset::{Stack, StackNode};
    use tddy_core::session_lifecycle::unified_session_dir_path;
    use tddy_core::{read_changeset, write_changeset, Changeset};

    /// A planned node: it carries the branch name the planner proposed, but nothing created it yet.
    fn a_planned_node(node_id: &str, branch_suggestion: &str, parents: &[&str]) -> StackNode {
        StackNode {
            node_id: node_id.to_string(),
            title: node_id.to_string(),
            description: String::new(),
            branch_suggestion: Some(branch_suggestion.to_string()),
            branch: None,
            session_id: None,
            parents: parents.iter().map(|p| p.to_string()).collect(),
            pr_status: None,
            child_state: None,
            internal_status: None,
            display_order: None,
        }
    }

    /// A pr-stack orchestrator whose planned stack is `bottom` (`feature/bottom`) → `top`.
    fn an_orchestrator_with_a_two_node_stack(sessions_base: &std::path::Path) -> PathBuf {
        let dir = unified_session_dir_path(sessions_base, "orchestrator-1");
        std::fs::create_dir_all(&dir).expect("create orchestrator session dir");
        write_changeset(
            &dir,
            &Changeset {
                recipe: Some("pr-stack".to_string()),
                stack: Some(Stack {
                    version: 1,
                    nodes: vec![
                        a_planned_node("bottom", "feature/bottom", &[]),
                        a_planned_node("top", "feature/top", &["bottom"]),
                    ],
                }),
                ..Changeset::default()
            },
        )
        .expect("write orchestrator changeset");
        dir
    }

    /// The same stack, except `bottom` already owns a branch that differs from its suggestion.
    fn an_orchestrator_with_a_renamed_bottom_branch(sessions_base: &std::path::Path) -> PathBuf {
        let dir = unified_session_dir_path(sessions_base, "orchestrator-1");
        std::fs::create_dir_all(&dir).expect("create orchestrator session dir");
        write_changeset(
            &dir,
            &Changeset {
                recipe: Some("pr-stack".to_string()),
                stack: Some(Stack {
                    version: 1,
                    nodes: vec![
                        StackNode {
                            branch: Some("feature/bottom-renamed".to_string()),
                            ..a_planned_node("bottom", "feature/bottom", &[])
                        },
                        a_planned_node("top", "feature/top", &["bottom"]),
                    ],
                }),
                ..Changeset::default()
            },
        )
        .expect("write orchestrator changeset");
        dir
    }

    /// The same stack, except `bottom` was worked on and then lost its session: it owns
    /// `feature/bottom`, but the session recorded against it no longer exists. This is the state
    /// `DeleteSession` leaves behind, and the one the operator recovers from.
    fn an_orchestrator_whose_bottom_node_lost_its_child_session(
        sessions_base: &std::path::Path,
    ) -> PathBuf {
        let dir = unified_session_dir_path(sessions_base, "orchestrator-1");
        std::fs::create_dir_all(&dir).expect("create orchestrator session dir");
        write_changeset(
            &dir,
            &Changeset {
                recipe: Some("pr-stack".to_string()),
                stack: Some(Stack {
                    version: 1,
                    nodes: vec![
                        StackNode {
                            branch: Some("feature/bottom".to_string()),
                            session_id: Some("deleted-child".to_string()),
                            ..a_planned_node("bottom", "feature/bottom", &[])
                        },
                        a_planned_node("top", "feature/top", &["bottom"]),
                    ],
                }),
                ..Changeset::default()
            },
        )
        .expect("write orchestrator changeset");
        dir
    }

    /// The same stack, except `bottom` never recorded its branch — only the child session that
    /// created it knows the name. Models a link written before the branch was known.
    fn an_orchestrator_whose_bottom_branch_only_its_session_knows(
        sessions_base: &std::path::Path,
    ) -> PathBuf {
        let child_dir = unified_session_dir_path(sessions_base, "child-1");
        std::fs::create_dir_all(&child_dir).expect("create child session dir");
        write_changeset(
            &child_dir,
            &Changeset {
                branch: Some("feature/bottom".to_string()),
                ..Changeset::default()
            },
        )
        .expect("write child changeset");

        let dir = unified_session_dir_path(sessions_base, "orchestrator-1");
        std::fs::create_dir_all(&dir).expect("create orchestrator session dir");
        write_changeset(
            &dir,
            &Changeset {
                recipe: Some("pr-stack".to_string()),
                stack: Some(Stack {
                    version: 1,
                    nodes: vec![
                        StackNode {
                            session_id: Some("child-1".to_string()),
                            ..a_planned_node("bottom", "feature/bottom", &[])
                        },
                        a_planned_node("top", "feature/top", &["bottom"]),
                    ],
                }),
                ..Changeset::default()
            },
        )
        .expect("write orchestrator changeset");
        dir
    }

    #[test]
    fn spawning_bases_a_node_on_a_parent_branch_only_its_child_session_recorded() {
        // Given — `bottom` owns no branch of its own; only its child session names one
        let tmp = tempfile::tempdir().expect("temp dir");
        an_orchestrator_whose_bottom_branch_only_its_session_knows(tmp.path());

        // When — `top` is spawned and the daemon resolves the node it materializes
        let (_dir, stack, node_id) =
            tddy_core::pr_stack_node_for_spawn(tmp.path(), "orchestrator-1", "feature/top")
                .expect("the planned node for the spawned branch must resolve");

        // Then — the session fallback supplies the parent's branch, so `top` is not blocked
        assert_eq!(node_id, "top");
        assert_eq!(
            stack
                .base_ref_for_spawn(&node_id, "origin/master")
                .expect("a session-resolved parent branch must unblock its dependent"),
            "origin/feature/bottom"
        );
    }

    fn stack_of(orchestrator_dir: &std::path::Path) -> Stack {
        read_changeset(orchestrator_dir)
            .expect("read orchestrator changeset")
            .stack
            .expect("orchestrator must carry a stack")
    }

    #[test]
    fn linking_records_the_branch_a_child_created_on_the_planned_node_it_materializes() {
        // Given — a planned stack, nothing spawned yet
        let tmp = tempfile::tempdir().expect("temp dir");
        let orchestrator_dir = an_orchestrator_with_a_two_node_stack(tmp.path());

        // When — a child creates the branch the bottom node was planned to own
        ConnectionServiceImpl::link_stack_node_to_spawned_branch(
            tmp.path(),
            Some("orchestrator-1"),
            "feature/bottom",
            "child-1",
        )
        .expect("linking the spawned branch must succeed");

        // Then — the node owns a real branch now; the sibling stays planned
        let stack = stack_of(&orchestrator_dir);
        assert_eq!(
            stack.node("bottom").and_then(|n| n.branch.as_deref()),
            Some("feature/bottom"),
            "the planned node must record the branch the child actually created"
        );
        assert_eq!(
            stack.node("top").and_then(|n| n.branch.as_deref()),
            None,
            "linking one node must not touch its siblings"
        );
    }

    #[test]
    fn linking_records_the_child_session_as_a_fallback_route_to_the_branch() {
        // Given — a planned stack, nothing spawned yet
        let tmp = tempfile::tempdir().expect("temp dir");
        let orchestrator_dir = an_orchestrator_with_a_two_node_stack(tmp.path());

        // When — a child creates the bottom node's branch
        ConnectionServiceImpl::link_stack_node_to_spawned_branch(
            tmp.path(),
            Some("orchestrator-1"),
            "feature/bottom",
            "child-1",
        )
        .expect("linking the spawned branch must succeed");

        // Then — the session is recorded too, so the branch stays resolvable from the session alone
        assert_eq!(
            stack_of(&orchestrator_dir)
                .node("bottom")
                .and_then(|n| n.session_id.as_deref()),
            Some("child-1")
        );
    }

    #[test]
    fn linking_a_planned_node_unblocks_spawning_its_dependent_node() {
        // Given — a planned stack where `top` depends on `bottom`
        let tmp = tempfile::tempdir().expect("temp dir");
        let orchestrator_dir = an_orchestrator_with_a_two_node_stack(tmp.path());

        // Before the link, `top` has no ref to base onto: the failed_precondition operators hit
        let err = stack_of(&orchestrator_dir)
            .base_ref_for_spawn("top", "origin/master")
            .expect_err("a parent without a branch must block its dependent");
        assert!(
            err.to_string().contains("no branch"),
            "unexpected error: {err}"
        );

        // When — `bottom`'s branch is created and linked
        ConnectionServiceImpl::link_stack_node_to_spawned_branch(
            tmp.path(),
            Some("orchestrator-1"),
            "feature/bottom",
            "child-1",
        )
        .expect("linking the spawned branch must succeed");

        // Then — `top` bases off its parent's branch instead of being refused
        assert_eq!(
            stack_of(&orchestrator_dir)
                .base_ref_for_spawn("top", "origin/master")
                .expect("a branch-owning parent must no longer block its dependent"),
            "origin/feature/bottom"
        );
    }

    #[test]
    fn linking_is_a_no_op_when_the_spawn_materializes_no_planned_node() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let orchestrator_dir = an_orchestrator_with_a_two_node_stack(tmp.path());

        // No stack parent at all, and a branch no planned node claims: both are ordinary sessions.
        ConnectionServiceImpl::link_stack_node_to_spawned_branch(
            tmp.path(),
            None,
            "feature/bottom",
            "child-1",
        )
        .expect("a parentless spawn must not error");
        ConnectionServiceImpl::link_stack_node_to_spawned_branch(
            tmp.path(),
            Some("orchestrator-1"),
            "feature/unplanned",
            "child-1",
        )
        .expect("a spawn whose branch matches no node must not error");

        let stack = stack_of(&orchestrator_dir);
        assert!(
            stack.nodes.iter().all(|n| n.branch.is_none()),
            "no planned node was materialized, so none may be linked"
        );
    }

    #[test]
    fn linking_repoints_a_node_to_the_child_session_that_now_owns_its_branch() {
        // Given — the bottom node's branch was first created by child-1
        let tmp = tempfile::tempdir().expect("temp dir");
        let orchestrator_dir = an_orchestrator_with_a_two_node_stack(tmp.path());
        ConnectionServiceImpl::link_stack_node_to_spawned_branch(
            tmp.path(),
            Some("orchestrator-1"),
            "feature/bottom",
            "child-1",
        )
        .expect("first link must succeed");

        // When — that session is replaced by a new one on the same branch (restart, re-attach)
        ConnectionServiceImpl::link_stack_node_to_spawned_branch(
            tmp.path(),
            Some("orchestrator-1"),
            "feature/bottom",
            "child-2",
        )
        .expect("a new session on the same branch must be accepted");

        // Then — the fallback points at the live session; the branch is untouched
        let stack = stack_of(&orchestrator_dir);
        assert_eq!(
            stack.node("bottom").and_then(|n| n.session_id.as_deref()),
            Some("child-2")
        );
        assert_eq!(
            stack.node("bottom").and_then(|n| n.branch.as_deref()),
            Some("feature/bottom")
        );
    }

    #[test]
    fn a_spawn_resuming_an_existing_branch_relinks_the_node_that_owns_it() {
        // Given — `bottom` owns a pushed branch whose session was deleted; the operator restarts it
        // with `work_on_selected_branch`, so no new branch is created and `new_branch_name` is empty
        let tmp = tempfile::tempdir().expect("temp dir");
        let orchestrator_dir = an_orchestrator_whose_bottom_node_lost_its_child_session(tmp.path());

        // When — the spawn links its node on the branch it actually operates on
        ConnectionServiceImpl::link_stack_node_to_spawned_branch(
            tmp.path(),
            Some("orchestrator-1"),
            effective_spawn_branch("work_on_selected_branch", "", "feature/bottom", "origin"),
            "child-2",
        )
        .expect("a resumed branch must link its planned node");

        // Then — the recovery sticks: the node points at the live session instead of the deleted one,
        // so the row leaves its recovered state and a second click cannot spawn another orphan
        let stack = stack_of(&orchestrator_dir);
        assert_eq!(
            (
                stack.node("bottom").and_then(|n| n.session_id.as_deref()),
                stack.node("bottom").and_then(|n| n.branch.as_deref())
            ),
            (Some("child-2"), Some("feature/bottom"))
        );
    }

    #[test]
    fn a_spawn_resuming_a_remote_tracking_branch_relinks_the_node_that_owns_it() {
        // Given — the same recovery, driven from the web: the dialog's branch picker is fed by
        // `ListProjectBranches`, which offers `origin/<branch>` names, so that is what the spawn carries
        let tmp = tempfile::tempdir().expect("temp dir");
        let orchestrator_dir = an_orchestrator_whose_bottom_node_lost_its_child_session(tmp.path());

        // When
        ConnectionServiceImpl::link_stack_node_to_spawned_branch(
            tmp.path(),
            Some("orchestrator-1"),
            effective_spawn_branch(
                "work_on_selected_branch",
                "",
                "origin/feature/bottom",
                "origin",
            ),
            "child-2",
        )
        .expect("a resumed remote-tracking branch must link its planned node");

        // Then — nodes record local branch names, so a prefixed key would match nothing and leave the
        // node orphaned forever
        let stack = stack_of(&orchestrator_dir);
        assert_eq!(
            (
                stack.node("bottom").and_then(|n| n.session_id.as_deref()),
                stack.node("bottom").and_then(|n| n.branch.as_deref())
            ),
            (Some("child-2"), Some("feature/bottom"))
        );
    }

    #[test]
    fn linking_matches_a_node_by_the_branch_it_recorded_rather_than_its_suggestion() {
        // Given — `bottom` was materialized on a branch other than the one the planner suggested
        let tmp = tempfile::tempdir().expect("temp dir");
        let orchestrator_dir = an_orchestrator_with_a_renamed_bottom_branch(tmp.path());

        // When — a session attaches to the branch the node actually owns
        ConnectionServiceImpl::link_stack_node_to_spawned_branch(
            tmp.path(),
            Some("orchestrator-1"),
            "feature/bottom-renamed",
            "child-1",
        )
        .expect("the recorded branch must identify the node");

        // Then — the node is found by its real branch, not missed because of its stale suggestion
        assert_eq!(
            stack_of(&orchestrator_dir)
                .node("bottom")
                .and_then(|n| n.session_id.as_deref()),
            Some("child-1")
        );
    }
}

#[cfg(test)]
mod cross_daemon_session_token_acceptance_tests {
    use super::*;

    /// A daemon config with GitHub auth enabled and, when `api_secret` is `Some`, a LiveKit
    /// secret that signs/verifies session tokens. Maps GitHub login "u" to OS user "u".
    fn a_daemon_config(
        api_secret: Option<&str>,
    ) -> (crate::config::DaemonConfig, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let livekit = match api_secret {
            Some(s) => format!("livekit:\n  api_secret: \"{s}\"\n"),
            None => String::new(),
        };
        let yaml = format!(
            "users:\n  - github_user: \"u\"\n    os_user: \"u\"\ngithub:\n  stub: true\n{livekit}"
        );
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        let config = crate::config::DaemonConfig::load(&path).unwrap();
        (config, dir)
    }

    fn a_github_user(login: &str) -> tddy_github::GitHubUser {
        tddy_github::GitHubUser {
            id: 7,
            login: login.to_string(),
            avatar_url: format!("https://github.com/{login}.png"),
            name: login.to_string(),
        }
    }

    /// A ConnectionService whose `user_resolver` is exactly the one the daemon's auth wiring
    /// produces for `config` — i.e. what a *peer* daemon verifies incoming tokens with.
    fn a_peer_daemon(
        config: crate::config::DaemonConfig,
        data_dir: std::path::PathBuf,
    ) -> ConnectionServiceImpl {
        let resolver = crate::auth::build_auth_entries(&config, "127.0.0.1", 0)
            .expect("auth wiring should build")
            .user_resolver
            .expect("auth wiring should produce a session resolver");
        let base = data_dir.clone();
        let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
        ConnectionServiceImpl::new(
            config,
            sessions_base_resolver,
            data_dir,
            resolver,
            None,
            None,
            None,
            Arc::new(CliSessionManager::new()),
        )
    }

    #[tokio::test]
    async fn a_peer_daemon_accepts_a_token_minted_with_the_same_shared_secret() {
        // Given a peer daemon whose auth is wired with a shared signing secret
        let (config, dir) = a_daemon_config(Some("shared-secret"));
        let service = a_peer_daemon(config, dir.path().to_path_buf());
        // and a token minted by another daemon holding that same secret
        let signer = tddy_github::SessionTokenSigner::new(b"shared-secret");
        let token = signer.mint(&a_github_user("u"), tddy_github::SESSION_TOKEN_TTL);

        // When the peer lists projects with that token
        let request = Request::new(ListProjectsRequest {
            session_token: token,
            local_only: true,
        });
        let result = service.list_projects(request).await;

        // Then the peer accepts it — no "invalid or expired session"
        assert!(
            result.is_ok(),
            "peer daemon should accept a token signed with the shared secret"
        );
    }

    #[tokio::test]
    async fn a_peer_daemon_rejects_a_token_signed_with_a_different_secret() {
        // Given a peer daemon wired with one signing secret
        let (config, dir) = a_daemon_config(Some("this-daemons-secret"));
        let service = a_peer_daemon(config, dir.path().to_path_buf());
        // and a token minted with a different secret
        let foreign = tddy_github::SessionTokenSigner::new(b"some-other-secret");
        let token = foreign.mint(&a_github_user("u"), tddy_github::SESSION_TOKEN_TTL);

        // When the peer lists projects with that token
        let request = Request::new(ListProjectsRequest {
            session_token: token,
            local_only: true,
        });
        let result = service.list_projects(request).await;

        // Then the peer rejects it as an invalid session
        let err = result.expect_err("a token signed with a foreign secret must be rejected");
        assert_eq!(err.code, tddy_rpc::Code::Unauthenticated);
    }
}

#[cfg(test)]
mod list_agent_models_parse_tests {
    use super::parse_agent_models_json;

    #[test]
    fn reads_the_models_and_default_from_the_tools_json() {
        // Given — the JSON contract emitted by `tddy-tools list-models`
        let stdout = r#"{"models":[{"id":"opus","label":"Claude Opus"},{"id":"sonnet","label":"Claude Sonnet"}],"default_model":"opus"}"#;

        // When
        let resp = parse_agent_models_json(stdout).expect("well-formed catalog should parse");

        // Then
        assert_eq!(resp.default_model, "opus");
        let ids: Vec<&str> = resp.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["opus", "sonnet"]);
        assert_eq!(resp.models[0].label, "Claude Opus");
    }

    #[test]
    fn errors_on_malformed_probe_output() {
        // When / Then — garbage is a hard error, never an empty catalog
        assert!(parse_agent_models_json("not json at all").is_err());
    }
}

#[cfg(test)]
mod start_session_binary_resolution_tests {
    use super::*;

    fn a_config_with_claude_binary(binary_path: &str) -> crate::config::DaemonConfig {
        let yaml = format!(
            "users:\n  - github_user: u\n    os_user: u\nclaude_cli:\n  binary_path: {binary_path}\n"
        );
        serde_yaml::from_str(&yaml).expect("daemon config should parse")
    }

    /// The interactive (non-sandboxed) StartSession path must resolve `claude` through the same
    /// host resolver as the sandboxed path, so an explicitly configured absolute path is honored
    /// instead of being spawned by bare name against the daemon's minimal systemd `PATH`.
    #[test]
    fn start_session_honors_an_explicitly_configured_claude_binary_path() {
        // Given a daemon config naming an explicit claude binary path
        let config = a_config_with_claude_binary("/opt/custom/bin/claude");

        // When resolving the binary for an interactive StartSession
        let resolved = resolve_start_session_claude_binary(&config);

        // Then the configured absolute path is used verbatim
        assert_eq!(resolved, "/opt/custom/bin/claude");
    }

    /// The StartSession resolver is the *same* resolution the sandboxed path uses — the two spawn
    /// paths must never diverge on which `claude` they pick.
    #[test]
    fn start_session_resolves_the_same_binary_as_the_sandbox_path() {
        // Given a daemon config naming an explicit claude binary path
        let config = a_config_with_claude_binary("/opt/custom/bin/claude");

        // When resolving via the StartSession path and the shared sandbox resolver
        let start_session = resolve_start_session_claude_binary(&config);
        let sandbox = crate::config::resolve_claude_binary_path(&config);

        // Then both paths agree
        assert_eq!(start_session, sandbox);
    }
}

#[cfg(test)]
mod resume_session_binary_resolution_tests {
    use super::*;

    fn a_config_with_claude_binary(binary_path: &str) -> crate::config::DaemonConfig {
        let yaml = format!(
            "users:\n  - github_user: u\n    os_user: u\nclaude_cli:\n  binary_path: {binary_path}\n"
        );
        serde_yaml::from_str(&yaml).expect("daemon config should parse")
    }

    /// A daemon config that leaves `binary_path` at its default (the bare name `claude`). This is
    /// the production case that broke ResumeSession: the bare name was spawned against the daemon's
    /// minimal systemd `PATH` (which omits `~/.local/bin`) instead of being resolved to a host path.
    fn a_config_with_default_claude_binary() -> crate::config::DaemonConfig {
        let yaml = "users:\n  - github_user: u\n    os_user: u\nclaude_cli: {}\n";
        serde_yaml::from_str(yaml).expect("daemon config should parse")
    }

    /// ResumeSession must resolve `claude` through the same host resolver as StartSession — an
    /// explicitly configured absolute path is honored verbatim, never spawned by bare name.
    #[test]
    fn resume_session_honors_an_explicitly_configured_claude_binary_path() {
        // Given a daemon config naming an explicit claude binary path
        let config = a_config_with_claude_binary("/opt/custom/bin/claude");

        // When resolving the binary for a ResumeSession relaunch
        let resolved = resolve_resume_session_claude_binary(&config);

        // Then the configured absolute path is used verbatim
        assert_eq!(resolved, "/opt/custom/bin/claude");
    }

    /// The ResumeSession resolver must be the *same* resolution StartSession uses — resume was the
    /// odd path out, spawning the bare config name while create resolved it to a host path.
    #[test]
    fn resume_session_resolves_the_same_binary_as_start_session() {
        // Given a daemon config that leaves the claude binary at its bare-name default
        let config = a_config_with_default_claude_binary();

        // When resolving via the ResumeSession path and the StartSession path
        let resume = resolve_resume_session_claude_binary(&config);
        let start_session = resolve_start_session_claude_binary(&config);

        // Then both paths pick the same binary — resume never diverges to the bare name
        assert_eq!(resume, start_session);
    }
}

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
mod worktree_source_tests {
    use super::{session_worktree_source, WorktreeSource};
    use std::path::PathBuf;

    #[test]
    fn uses_the_client_repo_path_when_present() {
        // Given — a request carrying an explicit local repo path
        // When
        let source = session_worktree_source("/home/dev/proj", "proj-123");

        // Then
        assert_eq!(
            source,
            WorktreeSource::RepoPath(PathBuf::from("/home/dev/proj"))
        );
    }

    #[test]
    fn falls_back_to_project_id_when_repo_path_is_empty() {
        // Given — no repo_path
        // When
        let source = session_worktree_source("", "proj-123");

        // Then
        assert_eq!(source, WorktreeSource::Project("proj-123".to_string()));
    }
}

#[cfg(test)]
mod sandbox_claude_passthrough_args_tests {
    use super::sandbox_claude_passthrough_args;

    #[test]
    fn forwards_client_claude_args_verbatim() {
        // Given — a client that passed extra claude flags and no prompt
        let claude_args = vec!["--add-dir".to_string(), "/repo/extra".to_string()];

        // When
        let tokens = sandbox_claude_passthrough_args(&claude_args, "");

        // Then
        assert_eq!(tokens, vec!["--add-dir", "/repo/extra"]);
    }

    #[test]
    fn appends_the_initial_prompt_last_as_a_trailing_positional() {
        // Given — both extra flags and an initial prompt
        let claude_args = vec!["--add-dir".to_string(), "/repo/extra".to_string()];

        // When
        let tokens = sandbox_claude_passthrough_args(&claude_args, "build feature X");

        // Then — the prompt must land after every flag so it stays a positional
        assert_eq!(tokens, vec!["--add-dir", "/repo/extra", "build feature X"]);
    }

    #[test]
    fn omits_a_blank_initial_prompt() {
        // Given — no client args and a whitespace-only prompt
        // When
        let tokens = sandbox_claude_passthrough_args(&[], "   ");

        // Then
        assert!(tokens.is_empty());
    }
}

#[cfg(test)]
mod terminal_output_chunking_tests {
    use super::{chunk_terminal_output, TERMINAL_OUTPUT_FRAME_MAX_BYTES};

    /// A terminal capture with a recognizable, order-sensitive byte pattern so that a bug which
    /// drops, duplicates, or reorders a chunk is caught on reassembly.
    fn a_terminal_capture_of(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    /// Concatenate the emitted frames back into a single buffer.
    fn reassembled(frames: &[bytes::Bytes]) -> Vec<u8> {
        frames.iter().flat_map(|f| f.iter().copied()).collect()
    }

    #[test]
    fn returns_no_frames_for_an_empty_capture() {
        // Given — a freshly attached session whose capture buffer is empty
        let capture = a_terminal_capture_of(0);

        // When
        let frames = chunk_terminal_output(&capture, 4);

        // Then — nothing to replay means no frame is published
        assert_eq!(frames, Vec::<bytes::Bytes>::new());
    }

    #[test]
    fn keeps_a_capture_that_fits_within_the_limit_as_one_frame() {
        // Given — a 3-byte capture and a 4-byte frame limit
        let capture = a_terminal_capture_of(3);

        // When
        let frames = chunk_terminal_output(&capture, 4);

        // Then — a single frame carrying the whole capture verbatim
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_ref(), capture.as_slice());
    }

    #[test]
    fn keeps_a_capture_exactly_at_the_limit_as_one_frame() {
        // Given — a capture whose length equals the frame limit
        let capture = a_terminal_capture_of(4);

        // When
        let frames = chunk_terminal_output(&capture, 4);

        // Then — the boundary case is not split
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_ref(), capture.as_slice());
    }

    #[test]
    fn splits_a_capture_larger_than_the_limit_into_multiple_frames() {
        // Given — a 10-byte capture and a 4-byte frame limit
        let capture = a_terminal_capture_of(10);

        // When
        let frames = chunk_terminal_output(&capture, 4);

        // Then — 4 + 4 + 2 = three frames, never one oversized frame
        assert_eq!(frames.len(), 3);
    }

    #[test]
    fn never_emits_a_frame_larger_than_the_limit() {
        // Given — a 10-byte capture and a 4-byte frame limit
        let capture = a_terminal_capture_of(10);

        // When
        let frames = chunk_terminal_output(&capture, 4);

        // Then — every frame, including the remainder, respects the limit
        let sizes: Vec<usize> = frames.iter().map(|f| f.len()).collect();
        assert_eq!(sizes, vec![4, 4, 2]);
    }

    #[test]
    fn reassembling_the_frames_reproduces_the_original_capture() {
        // Given — a 10-byte capture and a 4-byte frame limit
        let capture = a_terminal_capture_of(10);

        // When
        let frames = chunk_terminal_output(&capture, 4);

        // Then — chunking preserves byte content and order exactly
        assert_eq!(reassembled(&frames), capture);
    }

    #[test]
    fn applies_the_default_frame_limit_so_a_long_session_is_not_replayed_as_one_frame() {
        // Given — a one-megabyte capture, representative of a long-lived interactive session
        let capture = a_terminal_capture_of(1024 * 1024);

        // When — chunked at the production default limit
        let frames = chunk_terminal_output(&capture, TERMINAL_OUTPUT_FRAME_MAX_BYTES);

        // Then — the history is split into fixed-size frames that reassemble losslessly
        let expected_frame_count = capture.len().div_ceil(TERMINAL_OUTPUT_FRAME_MAX_BYTES);
        assert_eq!(frames.len(), expected_frame_count);
        assert!(frames
            .iter()
            .all(|f| f.len() <= TERMINAL_OUTPUT_FRAME_MAX_BYTES));
        assert_eq!(reassembled(&frames), capture);
    }

    #[test]
    fn the_default_frame_limit_is_small_enough_to_chunk_a_megabyte_capture() {
        // A one-megabyte session history must yield more than one frame — the whole point of the
        // change is that an oversized single frame can exceed the data-channel message limit and
        // never reach the browser. This guards the constant against being set unhelpfully large.
        const _: () = {
            assert!(TERMINAL_OUTPUT_FRAME_MAX_BYTES > 0);
            assert!(TERMINAL_OUTPUT_FRAME_MAX_BYTES < 1024 * 1024);
        };
    }
}

#[cfg(test)]
mod sandbox_replay_tests {
    use super::{sandbox_replay_frames, TERMINAL_OUTPUT_FRAME_MAX_BYTES};
    use tddy_task::TerminalCapture;

    /// DECSET 1006 — SGR mouse encoding, the mode `GhosttyTerminal` gates mouse forwarding on.
    const SGR_MOUSE_ENCODING: &[u8] = b"\x1b[?1006h";

    /// A sandbox session that turned on mouse reporting at startup and has since produced far
    /// more output than the capture ring retains.
    fn a_long_running_sandbox_capture() -> TerminalCapture {
        let mut capture = TerminalCapture::new();
        capture.append(SGR_MOUSE_ENCODING);
        capture.append(&vec![b'A'; 3 * TerminalCapture::CAPTURE_LIMIT_BYTES]);
        capture
    }

    #[test]
    fn leads_the_sandbox_replay_with_the_mouse_modes_still_in_effect() {
        // Given a sandbox session whose ring has long since evicted the enabling DECSET
        let capture = a_long_running_sandbox_capture();

        // When a browser attaches and takes the replay frames
        let frames = sandbox_replay_frames(&capture, TERMINAL_OUTPUT_FRAME_MAX_BYTES);

        // Then the first frame re-enables mouse reporting, so the attaching terminal forwards
        // clicks and scrolls instead of silently dropping them
        assert_eq!(
            frames.first().map(|frame| frame
                .iter()
                .copied()
                .take(SGR_MOUSE_ENCODING.len())
                .collect::<Vec<u8>>()),
            Some(SGR_MOUSE_ENCODING.to_vec()),
        );
    }
}

#[cfg(test)]
mod conversation_spawn_wiring_tests {
    use super::recipe_enables_conversation_spawn;

    /// Only the grill-me recipe binds a conversation-spawn handler on its managed session; other
    /// recipes (a plain TDD session, or the PR-stack orchestrator which uses `spawn-child` instead)
    /// must not, so `spawn_conversation` is rejected there rather than silently spawning.
    #[test]
    fn grill_me_recipe_enables_conversation_spawn_but_others_do_not() {
        // Then
        assert!(
            recipe_enables_conversation_spawn("grill-me"),
            "grill-me must enable the conversation-spawn handler"
        );
        assert!(
            !recipe_enables_conversation_spawn("tdd"),
            "a plain tdd session must not enable the conversation-spawn handler"
        );
        assert!(
            !recipe_enables_conversation_spawn("pr-stack"),
            "pr-stack uses spawn-child, not spawn-conversation"
        );
    }
}

#[cfg(test)]
mod list_agent_models_probe_tests {
    use super::*;
    use std::path::Path;

    /// The cursor model probe must hand `tddy-tools` the **resolved absolute** cursor binary via
    /// `--cursor-cli-path`, exactly as the PTY spawn does. Otherwise `tddy-tools` builds a
    /// `CursorBackend` with the bare name `agent`, and the impersonated child's PATH lookup fails
    /// with "binary not found: agent" — the reported `[failed_precondition] model probe failed`.
    #[test]
    fn cursor_probe_args_carry_the_resolved_cursor_binary_path() {
        // Given — the daemon has resolved the real cursor binary to an absolute path
        let resolved = Path::new("/home/dev/.local/bin/agent");

        // When — building the args for the `tddy-tools list-models` cursor probe
        let args = list_models_probe_args("cursor", Some(resolved));

        // Then — the resolved absolute path is passed through to tddy-tools
        assert_eq!(
            args,
            vec![
                "list-models".to_string(),
                "--agent".to_string(),
                "cursor".to_string(),
                "--cursor-cli-path".to_string(),
                "/home/dev/.local/bin/agent".to_string(),
            ]
        );
    }

    /// A non-cursor agent's probe carries no cursor override — `--cursor-cli-path` is cursor-only.
    #[test]
    fn a_non_cursor_probe_omits_the_cursor_cli_path_flag() {
        // When — probing a claude agent, even if a cursor path happens to be resolvable
        let args = list_models_probe_args("claude", Some(Path::new("/home/dev/.local/bin/agent")));

        // Then — only the agent is passed; no cursor override leaks in
        assert_eq!(
            args,
            vec![
                "list-models".to_string(),
                "--agent".to_string(),
                "claude".to_string(),
            ]
        );
    }
}

#[cfg(test)]
mod remote_branch_push_gating_tests {
    use super::*;
    use std::time::Duration;

    /// A session dir whose changeset records `branch` but has not been pushed.
    fn a_session_with_unpushed_branch() -> tempfile::TempDir {
        let session = tempfile::tempdir().unwrap();
        let cs = tddy_core::changeset::Changeset {
            branch: Some("feature/x".to_string()),
            remote_pushed: false,
            ..Default::default()
        };
        tddy_core::write_changeset(session.path(), &cs).unwrap();
        session
    }

    /// The push is skipped for "work on existing branch" even when Create Remote Branch is ticked —
    /// only a freshly created branch is ever pushed. The worktree is not a git repo, so a broken
    /// guard that attempted the push would fail loudly instead of returning Ok.
    #[tokio::test]
    async fn push_is_skipped_for_work_on_selected_branch_intent() {
        // Given
        let session = a_session_with_unpushed_branch();
        let worktree = tempfile::tempdir().unwrap();

        // When
        let result = push_new_branch_to_origin_if_requested(
            true,
            tddy_core::changeset::BranchWorktreeIntent::WorkOnSelectedBranch,
            session.path(),
            worktree.path(),
            Duration::from_secs(10),
        )
        .await;

        // Then
        assert!(
            result.is_ok(),
            "gated call must succeed without pushing: {result:?}"
        );
        let after = tddy_core::read_changeset(session.path()).unwrap();
        assert!(
            !after.remote_pushed,
            "remote_pushed must stay false when intent is not new-branch"
        );
    }

    /// The push is skipped when the operator opts out (flag false), even for a new branch.
    #[tokio::test]
    async fn push_is_skipped_when_create_remote_branch_is_false() {
        // Given
        let session = a_session_with_unpushed_branch();
        let worktree = tempfile::tempdir().unwrap();

        // When
        let result = push_new_branch_to_origin_if_requested(
            false,
            tddy_core::changeset::BranchWorktreeIntent::NewBranchFromBase,
            session.path(),
            worktree.path(),
            Duration::from_secs(10),
        )
        .await;

        // Then
        assert!(
            result.is_ok(),
            "opt-out call must succeed without pushing: {result:?}"
        );
        let after = tddy_core::read_changeset(session.path()).unwrap();
        assert!(
            !after.remote_pushed,
            "remote_pushed must stay false when the flag is off"
        );
    }
}
