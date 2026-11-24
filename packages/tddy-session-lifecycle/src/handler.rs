//! Port the eight `session.SessionService` RPCs are implemented against.

use std::pin::Pin;

use async_trait::async_trait;
use futures_util::stream::Stream;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{
    CloseSessionEvent, CloseSessionPreview, CloseSessionRequest, PreviewCloseSessionRequest,
};
use tddy_service::proto::session::{
    ConnectSessionRequest, ConnectSessionResponse, DeleteSessionRequest, DeleteSessionResponse,
    EnsureProjectHomeSessionRequest, EnsureProjectHomeSessionResponse, GetWorktreeSnapshotRequest,
    GetWorktreeSnapshotResponse, ListSessionsRequest, ListSessionsResponse, RenameSessionRequest,
    RenameSessionResponse, ResumeSessionRequest, ResumeSessionResponse, SignalSessionRequest,
    SignalSessionResponse, StartSessionEvent, StartSessionRequest, StartSessionResponse,
};

/// Server-streaming output for [`SessionHandler::stream_start_session`].
pub type SessionStartEventStream =
    Pin<Box<dyn Stream<Item = Result<StartSessionEvent, Status>> + Send>>;

/// Server-streaming output for [`SessionHandler::close_session`].
pub type CloseSessionEventStream =
    Pin<Box<dyn Stream<Item = Result<CloseSessionEvent, Status>> + Send>>;

/// What the nine session-lifecycle RPCs need from the host process.
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

    /// Start, resume, or return the project's project-home session (`#project-home` 4/7).
    async fn ensure_project_home_session(
        &self,
        request: Request<EnsureProjectHomeSessionRequest>,
    ) -> Result<Response<EnsureProjectHomeSessionResponse>, Status>;

    async fn signal_session(
        &self,
        request: Request<SignalSessionRequest>,
    ) -> Result<Response<SignalSessionResponse>, Status>;

    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status>;

    /// Record or clear the session's name in its `.session.yaml`.
    async fn rename_session(
        &self,
        request: Request<RenameSessionRequest>,
    ) -> Result<Response<RenameSessionResponse>, Status>;

    async fn get_worktree_snapshot(
        &self,
        request: Request<GetWorktreeSnapshotRequest>,
    ) -> Result<Response<GetWorktreeSnapshotResponse>, Status>;

    /// What closing the session would lose (docs/ft/daemon/session-liveness.md § Closing a session).
    async fn preview_close_session(
        &self,
        request: Request<PreviewCloseSessionRequest>,
    ) -> Result<Response<CloseSessionPreview>, Status>;

    /// Terminate every agent tab of the session, then remove every worktree it owns, as one task.
    async fn close_session(
        &self,
        request: Request<CloseSessionRequest>,
    ) -> Result<Response<CloseSessionEventStream>, Status>;
}
