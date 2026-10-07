//! The session host as the agent topic's and the split topic's callbacks (see
//! [`AgentHostCallbacks`] and [`SplitHost`]).
//!
//! Each method forwards to the host method or impl that already does the work, so the topic and
//! every other caller take exactly one path.

mod session_room_opening;

use std::path::Path;
use std::sync::Arc;

use tddy_daemon_livekit::session_room::{
    OpenedSessionRoom, RemoteSnapshotSource, WorktreeSnapshot,
};
use tddy_rpc::{Request, Response, Status};
use tddy_sandbox_runner::ExecuteToolResponse;
use tddy_service::proto::exec_tools::ExecuteToolRequest;
use tddy_service::proto::session::{
    DeleteSessionRequest, DeleteSessionResponse, StartSessionRequest, StartSessionResponse,
};
use tddy_session_agents::session_agent_clone::HostedClone;
use tddy_session_files::attachment_progress::AttachmentProgressSink;

use super::agent_host_callbacks::AgentHostCallbacks;
use super::split_ports::{SplitHost, SplitSessionAgents, SplitSessionFiles};
use super::DaemonSessionHost;

#[async_trait::async_trait]
impl AgentHostCallbacks for DaemonSessionHost {
    async fn worktree_snapshot(
        &self,
        session_token: &str,
        codebase_session_id: &str,
        codebase_instance_id: &str,
    ) -> Result<WorktreeSnapshot, Status> {
        RemoteSnapshotSource::snapshot(
            self,
            session_token,
            codebase_session_id,
            codebase_instance_id,
        )
        .await
    }

    async fn run_exec_tool_locally(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> ExecuteToolResponse {
        DaemonSessionHost::run_exec_tool_locally(self, req, sessions_base, worktree_root).await
    }

    fn hosted_clone_for(&self, session_id: &str) -> Option<Arc<HostedClone>> {
        self.local_exec_tools().hosted_clone_for(session_id)
    }

    async fn run_hosted_clone_tool(
        &self,
        req: &ExecuteToolRequest,
        clone: &HostedClone,
    ) -> ExecuteToolResponse {
        self.local_exec_tools()
            .run_hosted_clone_tool(req, clone)
            .await
    }

    async fn ensure_session_room(
        &self,
        session_id: &str,
        session_dir: &Path,
        worktree_root: &Path,
    ) -> Result<Option<OpenedSessionRoom>, Status> {
        DaemonSessionHost::ensure_session_room(self, session_id, session_dir, worktree_root).await
    }
}

#[async_trait::async_trait]
impl SplitHost for DaemonSessionHost {
    async fn start_workspace_session(
        &self,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        self.start_session_core(req, progress).await
    }

    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        self.delete_session_at_session_coordinate(request).await
    }

    fn session_files(&self) -> Arc<SplitSessionFiles> {
        Arc::new(Arc::new(self.clone()).session_files_service())
    }

    fn session_agents(&self) -> Arc<SplitSessionAgents> {
        Arc::new(self.session_agents_service())
    }

    fn session_room_roster(&self) -> Result<tddy_rpc::MultiRpcService, Status> {
        Arc::new(self.clone()).session_room_roster()
    }
}
