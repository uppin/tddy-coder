//! The session host as the agent topic's callbacks (see [`AgentHostCallbacks`]).
//!
//! Each method forwards to the host method or impl that already does the work, so the topic and
//! every other caller take exactly one path.

mod session_room_opening;

use std::path::Path;
use std::sync::Arc;

use tddy_daemon_livekit::session_room::{
    OpenedSessionRoom, RemoteSnapshotSource, WorktreeSnapshot,
};
use tddy_rpc::Status;
use tddy_sandbox_runner::ExecuteToolResponse;
use tddy_service::proto::exec_tools::ExecuteToolRequest;
use tddy_session_agents::session_agent_clone::HostedClone;

use super::agent_host_callbacks::AgentHostCallbacks;
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
