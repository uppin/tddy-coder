//! Typed RPCs forwarded to a peer daemon over [`CommonRoom`]: encode the request, forward it, decode
//! the answer. Transport-neutral — the room forwards through whatever `PeerForwarder` it was built
//! with — so they live here rather than in the LiveKit plugin, which re-exports them under their old
//! `*_via_livekit` names until its last caller moves.

use std::time::Duration;

use prost::Message;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolChunk, ExecuteToolRequest};
use tddy_service::proto::project::{
    AddProjectToHostRequest, AddProjectToHostResponse, SetProjectAccountsRequest,
    SetProjectAccountsResponse, SetProjectDefaultBranchRequest, SetProjectDefaultBranchResponse,
};
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

/// Forward **StartSession** to another daemon in the common room, under the room's forward deadline.
///
/// Thin encode/decode wrapper around [`CommonRoom::forward_to_peer_within`].
pub async fn forward_start_session(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &StartSessionRequest,
) -> Result<StartSessionResponse, Status> {
    forward_start_session_within(room, peer_instance_id, request, room.forward_timeout()).await
}

/// [`forward_start_session`] with an explicit deadline.
///
/// A start the peer serves by cloning a project and cutting a worktree outlasts the ordinary forward
/// deadline; see [`CommonRoom::forward_to_peer_within`].
pub async fn forward_start_session_within(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &StartSessionRequest,
    deadline: Duration,
) -> Result<StartSessionResponse, Status> {
    let body = request.encode_to_vec();
    let out = room
        .forward_to_peer_within(
            peer_instance_id,
            "session.SessionService",
            "StartSession",
            body,
            deadline,
        )
        .await?;
    StartSessionResponse::decode(out.as_slice())
        .map_err(|e| Status::internal(format!("decode StartSessionResponse: {e}")))
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
        |bytes| {
            StartSessionEvent::decode(bytes.as_slice())
                .map_err(|e| Status::internal(format!("decode StartSessionEvent from peer: {e}")))
        },
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
