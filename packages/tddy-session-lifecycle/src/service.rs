//! Thin `session.SessionService` adapter over a [`SessionHandler`].

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{
    ConnectSessionRequest, ConnectSessionResponse, DeleteSessionRequest, DeleteSessionResponse,
    GetWorktreeSnapshotRequest, GetWorktreeSnapshotResponse, ListSessionsRequest,
    ListSessionsResponse, ResumeSessionRequest, ResumeSessionResponse, SessionService,
    SignalSessionRequest, SignalSessionResponse, StartSessionRequest, StartSessionResponse,
};

use crate::handler::{SessionHandler, SessionStartEventStream};

/// Thin `SessionService` adapter over a [`SessionHandler`].
pub struct SessionServiceImpl<H> {
    host: Arc<H>,
}

impl<H> SessionServiceImpl<H> {
    #[must_use]
    pub fn new(host: Arc<H>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl<H: SessionHandler + 'static> SessionService for SessionServiceImpl<H> {
    type StreamStartSessionStream = SessionStartEventStream;

    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        self.host.list_sessions(request).await
    }

    async fn start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        self.host.start_session(request).await
    }

    async fn stream_start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<Self::StreamStartSessionStream>, Status> {
        self.host.stream_start_session(request).await
    }

    async fn connect_session(
        &self,
        request: Request<ConnectSessionRequest>,
    ) -> Result<Response<ConnectSessionResponse>, Status> {
        self.host.connect_session(request).await
    }

    async fn resume_session(
        &self,
        request: Request<ResumeSessionRequest>,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        self.host.resume_session(request).await
    }

    async fn signal_session(
        &self,
        request: Request<SignalSessionRequest>,
    ) -> Result<Response<SignalSessionResponse>, Status> {
        self.host.signal_session(request).await
    }

    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        self.host.delete_session(request).await
    }

    async fn get_worktree_snapshot(
        &self,
        request: Request<GetWorktreeSnapshotRequest>,
    ) -> Result<Response<GetWorktreeSnapshotResponse>, Status> {
        self.host.get_worktree_snapshot(request).await
    }
}

/// Registers `session.SessionService` on the RPC transport.
#[must_use]
pub fn build_session_entry<H>(service: SessionServiceImpl<H>) -> tddy_rpc::ServiceEntry
where
    H: SessionHandler + 'static,
{
    use tddy_service::proto::session::SessionServiceServer;

    tddy_rpc::ServiceEntry {
        name: "session.SessionService",
        service: Arc::new(SessionServiceServer::new(service)) as Arc<dyn tddy_rpc::RpcService>,
    }
}
