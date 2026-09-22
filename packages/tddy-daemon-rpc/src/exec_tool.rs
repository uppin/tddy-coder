//! Family L — `exec_tools.ExecToolService`.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use livekit::prelude::Room;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxRegistry;
use tddy_host_service::multi_host::EligibleDaemonSource;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::exec_tools::{
    ExecuteToolChunk, ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest,
    ListExecToolsResponse, ListSessionToolCallsRequest, ListSessionToolCallsResponse,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::session_agent_clone::HostedAgentClones;
use tddy_task::{IdleTimeoutTracker, TaskRegistry};
use tddy_tool_engine::ExecToolHandler;
use tddy_worktree_service::stream::MpscResultStream;

/// The four exec-tool RPCs: the caller's identity, the peer a split session's tools are forwarded
/// to, and the three places a tool actually runs — this host's task registry, a workspace sandbox,
/// or a hosted agent clone.
// TODO(#carve 11): the fields are read by the moved bodies; drop this allow when they land.
#[allow(dead_code)]
pub struct ExecToolRpcHandler {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    tddy_data_dir: PathBuf,
    eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    common_room_livekit_room: Option<Arc<tokio::sync::RwLock<Option<Arc<Room>>>>>,
    idle_tracker: Option<Arc<IdleTimeoutTracker>>,
    hosted_agent_clones: Arc<HostedAgentClones>,
    task_registry: TaskRegistry,
    workspace_sandboxes: Arc<WorkspaceSandboxRegistry>,
}

impl ExecToolRpcHandler {
    /// A handler sharing `host`'s state — the same task registry, sandboxes and hosted clones, so
    /// a tool it runs is one the rest of the daemon can see.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        // TODO(#carve 11): clone the host's exec-tool fields once the host exposes them.
        let _ = host;
        todo!("ExecToolRpcHandler::from_host")
    }
}

#[async_trait]
impl ExecToolHandler for ExecToolRpcHandler {
    async fn execute_tool(
        &self,
        _request: Request<ExecuteToolRequest>,
    ) -> Result<Response<ExecuteToolResponse>, Status> {
        todo!("ExecToolRpcHandler::execute_tool")
    }

    async fn stream_execute_tool(
        &self,
        _request: Request<ExecuteToolRequest>,
    ) -> Result<Response<MpscResultStream<ExecuteToolChunk>>, Status> {
        todo!("ExecToolRpcHandler::stream_execute_tool")
    }

    async fn list_exec_tools(
        &self,
        _request: Request<ListExecToolsRequest>,
    ) -> Result<Response<ListExecToolsResponse>, Status> {
        todo!("ExecToolRpcHandler::list_exec_tools")
    }

    async fn list_session_tool_calls(
        &self,
        _request: Request<ListSessionToolCallsRequest>,
    ) -> Result<Response<ListSessionToolCallsResponse>, Status> {
        todo!("ExecToolRpcHandler::list_session_tool_calls")
    }
}
