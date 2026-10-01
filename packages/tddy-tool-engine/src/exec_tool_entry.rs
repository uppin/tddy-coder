//! Registers an [`exec_tools.ExecToolService`] implementation on the `tddy-rpc` transport.

use std::sync::Arc;

use tddy_rpc::RpcService;
use tddy_service::proto::exec_tools::{ExecToolService, ExecToolServiceServer};

/// The coordinate `exec_tools.proto`'s four methods are served at.
pub const EXEC_TOOL_SERVICE: &str = "exec_tools.ExecToolService";

/// The exec-tool RPCs an in-jail `tddy-tools` may relay to its host besides `ExecuteTool` (which
/// rides the runner's own poll-gated tool slot). Read by `tddy-sandbox-runner`'s relay and by the
/// daemon's host bridge, so the two cannot disagree about the name.
pub const IN_JAIL_RELAYABLE_EXEC_TOOLS: [(&str, &str); 1] =
    [(EXEC_TOOL_SERVICE, "ConversationWorktree")];

/// The `exec_tools.ExecToolService` entry a host's wiring layer registers.
#[must_use]
pub fn build_exec_tool_entry<S>(service: S) -> tddy_rpc::ServiceEntry
where
    S: ExecToolService + Send + Sync + 'static,
{
    tddy_rpc::ServiceEntry {
        name: EXEC_TOOL_SERVICE,
        service: Arc::new(ExecToolServiceServer::new(service)) as Arc<dyn RpcService>,
    }
}
