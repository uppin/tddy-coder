//! The port the daemon implements to start a coding agent for one conversation.

use std::path::PathBuf;

use tddy_agent_backend::{AcpAgentCommand, McpServerSpec};
use tddy_discovery::subagent::SubagentError;

use crate::catalog::CodingAgentDef;

/// Everything a launch needs to know about the conversation it serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodingAgentLaunch {
    pub def: CodingAgentDef,
    /// The session the conversation belongs to.
    pub session_id: String,
    /// The OS user the session runs as; the agent runs as this user.
    pub session_os_user: String,
    /// The worktree the agent works on.
    pub worktree: PathBuf,
    /// A managed session: native tools withdrawn, `mcp_servers` carry the session's exec tools.
    pub managed: bool,
    /// MCP servers handed to the agent — in a managed session, `tddy-tools --mcp` pointed at the
    /// session's exec tools.
    pub mcp_servers: Vec<McpServerSpec>,
}

/// Turns a [`CodingAgentLaunch`] into the command the ACP driver spawns: the harness binary,
/// wrapped to run as the session's OS user, with the provider account's route applied and, in a
/// managed session, its native tools withdrawn.
#[async_trait::async_trait]
pub trait CodingAgentLauncher: Send + Sync {
    async fn command_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError>;
}
