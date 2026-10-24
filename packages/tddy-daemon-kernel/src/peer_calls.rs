//! Typed RPCs forwarded to a peer daemon over [`CommonRoom`]: encode the request, forward it, decode
//! the answer. Transport-neutral — the room forwards through whatever `PeerForwarder` it was built
//! with — so they live here rather than in the LiveKit plugin, which re-exports them under their old
//! `*_via_livekit` names until its last caller moves.

use prost::Message;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolChunk, ExecuteToolRequest};
use tddy_service::proto::project::{
    AddProjectToHostRequest, AddProjectToHostResponse, SetProjectAccountsRequest,
    SetProjectAccountsResponse, SetProjectDefaultBranchRequest, SetProjectDefaultBranchResponse,
};
use tddy_service::proto::session::start_session_event::Event as StartSessionEventKind;
use tddy_service::proto::session::{
    DeleteSessionRequest, DeleteSessionResponse, StartSessionEvent, StartSessionRequest,
    StartSessionResponse,
};
use tddy_service::proto::session_files::{
    DeleteStagedAttachmentRequest, DeleteStagedAttachmentResponse, HostDocumentChunk,
    ListStagedAttachmentsRequest, ListStagedAttachmentsResponse, ReadHostDocumentRequest,
    ReadHostDocumentResponse, UploadStagedAttachmentChunkRequest,
    UploadStagedAttachmentChunkResponse,
};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::peer_forwarding::CommonRoom;
use crate::stream_relay::RelayFailure;

/// Encode `request`, forward it to `service`/`method` on the peer, and decode the answer as `Resp`;
/// an undecodable answer is `Internal`, naming the response type.
async fn forward_unary<Req: Message, Resp: Message + Default>(
    room: &CommonRoom,
    peer_instance_id: &str,
    service: &str,
    method: &str,
    request: &Req,
) -> Result<Resp, Status> {
    let out = room
        .forward_to_peer(peer_instance_id, service, method, request.encode_to_vec())
        .await?;
    Resp::decode(out.as_slice())
        .map_err(|e| Status::internal(format!("decode {}: {e}", short_type_name::<Resp>())))
}

/// `AddProjectToHostResponse` for `tddy_service::proto::project::AddProjectToHostResponse`.
fn short_type_name<T>() -> &'static str {
    let full = std::any::type_name::<T>();
    full.rsplit("::").next().unwrap_or(full)
}

/// Forward **StreamStartSession** to another daemon in the common room, yielding the peer's
/// start-session events (attachment progress, then the terminal result).
///
/// Thin decode wrapper around [`CommonRoom::forward_server_stream_to_peer`]. The session — and its
/// attachments — live on the peer; only the events cross back.
pub async fn forward_stream_start_session(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &StartSessionRequest,
) -> Result<UnboundedReceiver<Result<StartSessionEvent, Status>>, Status> {
    room.forward_server_stream_to_peer(
        peer_instance_id,
        "session.SessionService",
        "StreamStartSession",
        request.encode_to_vec(),
        decode_start_session_event,
    )
    .await
}

/// Why a forwarded start did not produce a session.
#[derive(Debug)]
pub enum ForwardedStartFailure {
    /// The peer answered with this status — whatever its code, including `DEADLINE_EXCEEDED` from
    /// the peer's own deadlines. The peer has said what happened.
    PeerAnswered(Status),
    /// The peer sent nothing for [`crate::peer_forwarding::PEER_FORWARD_STREAM_IDLE_TIMEOUT`]: it
    /// may still be working, so whatever it was building may still appear.
    PeerSilent(Status),
}

impl ForwardedStartFailure {
    /// The status to hand the caller.
    pub fn into_status(self) -> Status {
        match self {
            ForwardedStartFailure::PeerAnswered(status)
            | ForwardedStartFailure::PeerSilent(status) => status,
        }
    }
}

impl From<RelayFailure> for ForwardedStartFailure {
    fn from(failure: RelayFailure) -> Self {
        match failure {
            RelayFailure::Peer(status) => ForwardedStartFailure::PeerAnswered(status),
            RelayFailure::Silent(status) => ForwardedStartFailure::PeerSilent(status),
        }
    }
}

/// Forward a start to `peer_instance_id` as **StreamStartSession**, handing every non-terminal event
/// the peer sends (phases, keep-alives, progress) to `relay` as it arrives, and returning the peer's
/// result. There is no overall deadline: the relay's per-frame idle timeout ends a start whose peer
/// has gone silent, and a live peer keeps it open with keep-alives.
pub async fn forward_start_session_relaying(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &StartSessionRequest,
    relay: &(dyn Fn(StartSessionEvent) + Send + Sync),
) -> Result<StartSessionResponse, ForwardedStartFailure> {
    let mut events = room
        .forward_server_stream_classified(
            peer_instance_id,
            "session.SessionService",
            "StreamStartSession",
            request.encode_to_vec(),
            decode_start_session_event,
        )
        .await?;
    while let Some(item) = events.recv().await {
        match item {
            Ok(StartSessionEvent {
                event: Some(StartSessionEventKind::Result(response)),
            }) => return Ok(response),
            Ok(event) => relay(event),
            Err(failure) => return Err(failure.into()),
        }
    }
    // The peer closed its stream cleanly without a result: no session was reported, so the start failed.
    Err(ForwardedStartFailure::PeerAnswered(Status::internal(
        "the peer ended its start stream without a result",
    )))
}

fn decode_start_session_event(bytes: Vec<u8>) -> Result<StartSessionEvent, Status> {
    StartSessionEvent::decode(bytes.as_slice())
        .map_err(|e| Status::internal(format!("decode StartSessionEvent from peer: {e}")))
}

/// Forward **CancelTask** to the daemon that runs the task — a split start's codebase half, or a task a
/// client addressed to a peer by `daemon_instance_id`.
pub async fn forward_cancel_task(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &tddy_service::proto::tasks::CancelTaskRequest,
) -> Result<tddy_service::proto::tasks::CancelTaskResponse, Status> {
    forward_unary(
        room,
        peer_instance_id,
        "tasks.TaskService",
        "CancelTask",
        request,
    )
    .await
}

/// Forward **DeleteSession** to another daemon in the common room.
///
/// `DeleteSessionRequest` carries no `daemon_instance_id`, so the peer is addressed explicitly. Used
/// to delete the paired `workspace` session that holds a split session's worktree — including the
/// teardown of a split start that failed after the peer had already created it.
pub async fn forward_delete_session(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &DeleteSessionRequest,
) -> Result<DeleteSessionResponse, Status> {
    forward_unary(
        room,
        peer_instance_id,
        "session.SessionService",
        "DeleteSession",
        request,
    )
    .await
}

/// Forward **StreamExecuteTool** to another daemon in the common room, yielding the peer's result
/// frames.
///
/// Thin decode wrapper around [`CommonRoom::forward_server_stream_to_peer`]: a frame that fails to decode — or a
/// stream that stops without its `last` frame — terminates the relay with a status, so a caller
/// reassembling a tool result never mistakes a truncated one for the whole answer.
pub async fn forward_stream_execute_tool(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &ExecuteToolRequest,
) -> Result<UnboundedReceiver<Result<ExecuteToolChunk, Status>>, Status> {
    room.forward_server_stream_to_peer(
        peer_instance_id,
        "exec_tools.ExecToolService",
        "StreamExecuteTool",
        request.encode_to_vec(),
        |bytes| {
            ExecuteToolChunk::decode(bytes.as_slice())
                .map_err(|e| Status::internal(format!("decode ExecuteToolChunk from peer: {e}")))
        },
    )
    .await
}

/// Forward **AddProjectToHost** to another daemon in the common room.
///
/// Thin encode/decode wrapper around [`CommonRoom::forward_to_peer`].
pub async fn forward_add_project_to_host(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &AddProjectToHostRequest,
) -> Result<AddProjectToHostResponse, Status> {
    forward_unary(
        room,
        peer_instance_id,
        "project.ProjectService",
        "AddProjectToHost",
        request,
    )
    .await
}

/// Forward **SetProjectDefaultBranch** to another daemon in the common room.
///
/// Thin encode/decode wrapper around [`CommonRoom::forward_to_peer`].
pub async fn forward_set_project_default_branch(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &SetProjectDefaultBranchRequest,
) -> Result<SetProjectDefaultBranchResponse, Status> {
    forward_unary(
        room,
        peer_instance_id,
        "project.ProjectService",
        "SetProjectDefaultBranch",
        request,
    )
    .await
}

/// Forward **SetProjectAccounts** to another daemon in the common room.
///
/// Thin encode/decode wrapper around [`CommonRoom::forward_to_peer`].
pub async fn forward_set_project_accounts(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &SetProjectAccountsRequest,
) -> Result<SetProjectAccountsResponse, Status> {
    forward_unary(
        room,
        peer_instance_id,
        "project.ProjectService",
        "SetProjectAccounts",
        request,
    )
    .await
}

/// Forward **UploadStagedAttachmentChunk** to another daemon in the common room.
///
/// This and the four session-file forwarders below address
/// [`tddy_service::SESSION_FILES_SERVICE`] rather than spelling the coordinate out: the peer serves
/// it under that same constant, and a forward addressed at a name the peer does not serve fails on
/// the peer at runtime — see the constant's own documentation.
pub async fn forward_upload_staged_attachment_chunk(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &UploadStagedAttachmentChunkRequest,
) -> Result<UploadStagedAttachmentChunkResponse, Status> {
    forward_unary(
        room,
        peer_instance_id,
        tddy_service::SESSION_FILES_SERVICE,
        "UploadStagedAttachmentChunk",
        request,
    )
    .await
}

/// Forward **ListStagedAttachments** to another daemon in the common room.
pub async fn forward_list_staged_attachments(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &ListStagedAttachmentsRequest,
) -> Result<ListStagedAttachmentsResponse, Status> {
    forward_unary(
        room,
        peer_instance_id,
        tddy_service::SESSION_FILES_SERVICE,
        "ListStagedAttachments",
        request,
    )
    .await
}

/// Forward **DeleteStagedAttachment** to another daemon in the common room.
pub async fn forward_delete_staged_attachment(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &DeleteStagedAttachmentRequest,
) -> Result<DeleteStagedAttachmentResponse, Status> {
    forward_unary(
        room,
        peer_instance_id,
        tddy_service::SESSION_FILES_SERVICE,
        "DeleteStagedAttachment",
        request,
    )
    .await
}

/// Forward **ReadHostDocument** to another daemon in the common room.
pub async fn forward_read_host_document(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &ReadHostDocumentRequest,
) -> Result<ReadHostDocumentResponse, Status> {
    forward_unary(
        room,
        peer_instance_id,
        tddy_service::SESSION_FILES_SERVICE,
        "ReadHostDocument",
        request,
    )
    .await
}

/// Forward **StreamReadHostDocument** to another daemon in the common room, yielding the peer's
/// document frames.
///
/// Thin decode wrapper around [`CommonRoom::forward_server_stream_to_peer`]: a frame that fails to decode
/// terminates the stream with a status, so a caller reassembling the document never treats a
/// malformed frame as the document's end.
pub async fn forward_stream_read_host_document(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &ReadHostDocumentRequest,
) -> Result<UnboundedReceiver<Result<HostDocumentChunk, Status>>, Status> {
    room.forward_server_stream_to_peer(
        peer_instance_id,
        tddy_service::SESSION_FILES_SERVICE,
        "StreamReadHostDocument",
        request.encode_to_vec(),
        |bytes| {
            HostDocumentChunk::decode(bytes.as_slice())
                .map_err(|e| Status::internal(format!("decode HostDocumentChunk from peer: {e}")))
        },
    )
    .await
}
