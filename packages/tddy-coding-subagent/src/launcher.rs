//! The port the daemon implements to start a coding agent for one conversation.

use std::path::PathBuf;

use tddy_agent_backend::{AcpAgentCommand, McpServerSpec};
use tddy_discovery::subagent::SubagentError;

use crate::catalog::CodingAgentDef;

/// How the ACP driver reaches the agent a launcher started.
#[derive(Debug)]
pub enum AgentTransport {
    /// A command the driver spawns itself, as the daemon's own user (or already wrapped to switch).
    Command(AcpAgentCommand),
    /// A connected stream to an agent started elsewhere — by `tddy-supervisor` as the session's OS
    /// user, its stdio relayed over this stream. Closing it ends the agent.
    Stream(std::os::unix::net::UnixStream),
}

/// Everything a launch needs to know about the conversation it serves.
#[derive(Clone, PartialEq, Eq)]
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
    /// The caller's session token — what the launcher reads the session owner's vault and mints the
    /// agent's own tool credential with. Never logged.
    pub session_token: String,
}

impl std::fmt::Debug for CodingAgentLaunch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodingAgentLaunch")
            .field("def", &self.def)
            .field("session_id", &self.session_id)
            .field("session_os_user", &self.session_os_user)
            .field("worktree", &self.worktree)
            .field("managed", &self.managed)
            .field("mcp_servers", &self.mcp_servers)
            .field("session_token", &"<redacted>")
            .finish()
    }
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

    /// How the driver reaches the agent. By default the [`AcpAgentCommand`] of [`Self::command_for`];
    /// a launcher that starts the agent somewhere the driver cannot spawn (another user's, through
    /// a supervisor) overrides it to hand back a stream.
    async fn launch_transport(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AgentTransport, SubagentError> {
        self.command_for(launch).await.map(AgentTransport::Command)
    }

    /// The MCP servers the ACP session is opened with. By default the launch's own; a launcher that
    /// has to complete them (resolve the binary, mint the agent's tool credential) overrides it.
    async fn mcp_servers_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<Vec<McpServerSpec>, SubagentError> {
        Ok(launch.mcp_servers.clone())
    }
}
