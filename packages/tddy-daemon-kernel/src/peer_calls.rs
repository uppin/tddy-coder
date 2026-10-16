//! Typed RPCs forwarded to a peer daemon over [`CommonRoom`]: encode the request, forward it, decode
//! the answer. Transport-neutral — the room forwards through whatever `PeerForwarder` it was built
//! with — so they live here rather than in the LiveKit plugin, which re-exports them under their old
//! `*_via_livekit` names until its last caller moves.

use std::time::Duration;

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

pub async fn forward_start_session(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &StartSessionRequest,
) -> Result<StartSessionResponse, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_start_session")
}

pub async fn forward_start_session_within(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &StartSessionRequest,
    deadline: Duration,
) -> Result<StartSessionResponse, Status> {
    let _ = (room, peer_instance_id, request, deadline);
    todo!("peer_calls::forward_start_session_within")
}

pub async fn forward_stream_start_session(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &StartSessionRequest,
) -> Result<UnboundedReceiver<Result<StartSessionEvent, Status>>, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_stream_start_session")
}

pub async fn forward_delete_session(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &DeleteSessionRequest,
) -> Result<DeleteSessionResponse, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_delete_session")
}

pub async fn forward_stream_execute_tool(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &ExecuteToolRequest,
) -> Result<UnboundedReceiver<Result<ExecuteToolChunk, Status>>, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_stream_execute_tool")
}

pub async fn forward_add_project_to_host(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &AddProjectToHostRequest,
) -> Result<AddProjectToHostResponse, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_add_project_to_host")
}

pub async fn forward_set_project_default_branch(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &SetProjectDefaultBranchRequest,
) -> Result<SetProjectDefaultBranchResponse, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_set_project_default_branch")
}

pub async fn forward_set_project_accounts(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &SetProjectAccountsRequest,
) -> Result<SetProjectAccountsResponse, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_set_project_accounts")
}

pub async fn forward_upload_staged_attachment_chunk(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &UploadStagedAttachmentChunkRequest,
) -> Result<UploadStagedAttachmentChunkResponse, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_upload_staged_attachment_chunk")
}

pub async fn forward_list_staged_attachments(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &ListStagedAttachmentsRequest,
) -> Result<ListStagedAttachmentsResponse, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_list_staged_attachments")
}

pub async fn forward_delete_staged_attachment(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &DeleteStagedAttachmentRequest,
) -> Result<DeleteStagedAttachmentResponse, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_delete_staged_attachment")
}

pub async fn forward_read_host_document(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &ReadHostDocumentRequest,
) -> Result<ReadHostDocumentResponse, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_read_host_document")
}

pub async fn forward_stream_read_host_document(
    room: &CommonRoom,
    peer_instance_id: &str,
    request: &ReadHostDocumentRequest,
) -> Result<UnboundedReceiver<Result<HostDocumentChunk, Status>>, Status> {
    let _ = (room, peer_instance_id, request);
    todo!("peer_calls::forward_stream_read_host_document")
}
