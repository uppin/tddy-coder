use super::DaemonSessionHost;

use tddy_sandbox_runner::ExecuteToolResponse;

use std::path::Path;

use tddy_service::proto::exec_tools::ExecuteToolRequest;

impl DaemonSessionHost {
    /// [`LocalExecTools::run_exec_tool_locally`](super::LocalExecTools::run_exec_tool_locally) over this host's task registry and jails.
    ///
    /// The single choke point for every exec tool this daemon serves out of its own sessions —
    /// `ExecuteTool`, `StreamExecuteTool`, and a roster agent's own loop
    /// ([`Self::local_agent_codebase_access`]).
    pub(crate) async fn run_exec_tool_locally(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> ExecuteToolResponse {
        self.local_exec_tools()
            .run_exec_tool_locally(req, sessions_base, worktree_root)
            .await
    }
}
