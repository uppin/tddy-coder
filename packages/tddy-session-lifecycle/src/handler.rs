//! Port the eight `session.SessionService` RPCs are implemented against.

use std::pin::Pin;

use async_trait::async_trait;
use futures_util::stream::Stream;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{
    ConnectSessionRequest, ConnectSessionResponse, DeleteSessionRequest, DeleteSessionResponse,
    GetWorktreeSnapshotRequest, GetWorktreeSnapshotResponse, ListSessionsRequest,
    ListSessionsResponse, ResumeSessionRequest, ResumeSessionResponse, SignalSessionRequest,
    SignalSessionResponse, StartSessionEvent, StartSessionRequest, StartSessionResponse,
};

/// Server-streaming output for [`SessionHandler::stream_start_session`].
pub type SessionStartEventStream =
    Pin<Box<dyn Stream<Item = Result<StartSessionEvent, Status>> + Send>>;

/// What the eight session-lifecycle RPCs need from the host process.
#[async_trait]
pub trait SessionHandler: Send + Sync {
    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status>;

    async fn start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<StartSessionResponse>, Status>;

    async fn stream_start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<SessionStartEventStream>, Status>;

    async fn connect_session(
        &self,
        request: Request<ConnectSessionRequest>,
    ) -> Result<Response<ConnectSessionResponse>, Status>;

    async fn resume_session(
        &self,
        request: Request<ResumeSessionRequest>,
    ) -> Result<Response<ResumeSessionResponse>, Status>;

    async fn signal_session(
        &self,
        request: Request<SignalSessionRequest>,
    ) -> Result<Response<SignalSessionResponse>, Status>;

    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status>;

    async fn get_worktree_snapshot(
        &self,
        request: Request<GetWorktreeSnapshotRequest>,
    ) -> Result<Response<GetWorktreeSnapshotResponse>, Status>;
}
