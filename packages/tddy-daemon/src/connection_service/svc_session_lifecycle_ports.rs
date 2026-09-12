//! Family C — host side of [`tddy_session_lifecycle::SessionHandler`].

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_session_lifecycle::SessionHandler;

use super::ConnectionServiceImpl;

#[async_trait]
impl SessionHandler for ConnectionServiceImpl {
    async fn list_sessions(
        &self,
        request: Request<tddy_service::proto::session::ListSessionsRequest>,
    ) -> Result<Response<tddy_service::proto::session::ListSessionsResponse>, Status> {
        self.list_sessions_at_session_coordinate(request).await
    }

    async fn start_session(
        &self,
        request: Request<tddy_service::proto::session::StartSessionRequest>,
    ) -> Result<Response<tddy_service::proto::session::StartSessionResponse>, Status> {
        self.start_session_at_session_coordinate(request).await
    }

    async fn stream_start_session(
        &self,
        request: Request<tddy_service::proto::session::StartSessionRequest>,
    ) -> Result<Response<tddy_session_lifecycle::SessionStartEventStream>, Status> {
        use super::family_proto_bridge::wire_same;

        let response = self.stream_start_session_at_session_coordinate(request).await?;
        let rx = response.into_inner().into_receiver();
        let stream = futures_util::stream::unfold(rx, |mut rx| async {
            match rx.recv().await {
                Some(Ok(ev)) => match wire_same(&ev) {
                    Ok(session_ev) => Some((Ok(session_ev), rx)),
                    Err(status) => Some((Err(status), rx)),
                },
                Some(Err(status)) => Some((Err(status), rx)),
                None => None,
            }
        });
        Ok(Response::new(Box::pin(stream)))
    }

    async fn connect_session(
        &self,
        request: Request<tddy_service::proto::session::ConnectSessionRequest>,
    ) -> Result<Response<tddy_service::proto::session::ConnectSessionResponse>, Status> {
        self.connect_session_at_session_coordinate(request).await
    }

    async fn resume_session(
        &self,
        request: Request<tddy_service::proto::session::ResumeSessionRequest>,
    ) -> Result<Response<tddy_service::proto::session::ResumeSessionResponse>, Status> {
        self.resume_session_at_session_coordinate(request).await
    }

    async fn signal_session(
        &self,
        request: Request<tddy_service::proto::session::SignalSessionRequest>,
    ) -> Result<Response<tddy_service::proto::session::SignalSessionResponse>, Status> {
        self.signal_session_at_session_coordinate(request).await
    }

    async fn delete_session(
        &self,
        request: Request<tddy_service::proto::session::DeleteSessionRequest>,
    ) -> Result<Response<tddy_service::proto::session::DeleteSessionResponse>, Status> {
        self.delete_session_at_session_coordinate(request).await
    }

    async fn get_worktree_snapshot(
        &self,
        request: Request<tddy_service::proto::session::GetWorktreeSnapshotRequest>,
    ) -> Result<Response<tddy_service::proto::session::GetWorktreeSnapshotResponse>, Status> {
        self.get_worktree_snapshot_at_session_coordinate(request).await
    }
}

impl ConnectionServiceImpl {
    #[must_use]
    pub fn session_lifecycle_service(
        self: &std::sync::Arc<Self>,
    ) -> tddy_session_lifecycle::SessionServiceImpl<Self> {
        tddy_session_lifecycle::SessionServiceImpl::new(std::sync::Arc::clone(self))
    }

    #[must_use]
    pub fn session_lifecycle_entry(self: &std::sync::Arc<Self>) -> tddy_rpc::ServiceEntry {
        tddy_session_lifecycle::build_session_entry(self.session_lifecycle_service())
    }
}
