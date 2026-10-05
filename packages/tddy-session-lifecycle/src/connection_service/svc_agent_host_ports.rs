//! The session host as the agent topic's callbacks (see [`AgentHostCallbacks`]).
//!
//! Each method forwards to the host method or impl that already does the work, so the topic and
//! every other caller take exactly one path.

use std::path::Path;
use std::sync::Arc;

use tddy_daemon_livekit::session_room::{RemoteSnapshotSource, WorktreeSnapshot};
use tddy_rpc::{MultiRpcService, Status};
use tddy_sandbox_runner::ExecuteToolResponse;
use tddy_service::proto::exec_tools::ExecuteToolRequest;

use super::agent_host_callbacks::AgentHostCallbacks;
use super::{DaemonSessionHost, LocalExecTools};

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

    fn local_exec_tools(&self) -> LocalExecTools {
        DaemonSessionHost::local_exec_tools(self)
    }

    fn session_room_roster(&self) -> Result<MultiRpcService, Status> {
        DaemonSessionHost::session_room_roster(&Arc::new(self.clone()))
    }
}
