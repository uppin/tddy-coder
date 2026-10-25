//! The port the daemon implements to start a coding agent for one conversation.

use std::path::{Path, PathBuf};

use tddy_agent_backend::{AcpAgentCommand, HeadlessCommand, McpServerSpec};
use tddy_discovery::subagent::SubagentError;

use crate::catalog::CodingAgentDef;

/// The `name` the `tddy-tools` MCP server is registered under in a managed coding agent's session,
/// and the binary it runs.
pub const TDDY_TOOLS_SERVER: &str = "tddy-tools";
/// The argument that runs `tddy-tools` as an MCP server.
pub const TDDY_TOOLS_MCP_ARG: &str = "--mcp";
/// The environment variable naming the conversation a managed agent's `tddy-tools --mcp` serves
/// exec calls for. Set only for a conversation that has a worktree of its own; read by `tddy-tools`,
/// which then runs its exec calls in that worktree rather than the session's.
pub const CONVERSATION_ID_ENV: &str = "TDDY_CONVERSATION_ID";
/// The environment variable a managed agent's `tddy-tools --mcp` is handed its minted credential in.
/// A headless conversation's permission approvals are bound to the credential the launch hands out
/// under this name.
pub const SESSION_TOKEN_ENV: &str = "TDDY_REMOTE_SESSION_TOKEN";
/// The directory under a daemon's data directory a headless conversation keeps its files in: one
/// subdirectory per conversation, named by the conversation's id, holding the MCP config its CLI reads.
pub const HEADLESS_DIR: &str = "headless";

/// The directory a headless conversation `conversation_id` keeps its files in, under `tddy_data_dir`.
#[must_use]
pub fn headless_conversation_dir(tddy_data_dir: &Path, conversation_id: &str) -> PathBuf {
    tddy_data_dir.join(HEADLESS_DIR).join(conversation_id)
}

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
    /// A managed session: native tools withdrawn, and the launcher's
    /// [`CodingAgentLauncher::mcp_servers_for`] hands the agent the session's exec tools.
    pub managed: bool,
    /// The caller's session token — what the launcher reads the session owner's vault and mints the
    /// agent's own tool credential with. Never logged.
    pub session_token: String,
    /// The conversation this launch serves, when it has a worktree of its own: its exec calls then
    /// run in that worktree ([`CONVERSATION_ID_ENV`]). `None` runs them in the session's worktree.
    pub conversation_id: Option<String>,
}

impl std::fmt::Debug for CodingAgentLaunch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodingAgentLaunch")
            .field("def", &self.def)
            .field("session_id", &self.session_id)
            .field("session_os_user", &self.session_os_user)
            .field("worktree", &self.worktree)
            .field("managed", &self.managed)
            .field("conversation_id", &self.conversation_id)
            .field("session_token", &"<redacted>")
            .finish()
    }
}

/// Turns a [`CodingAgentLaunch`] into the command the ACP driver spawns: the harness binary,
/// wrapped to run as the session's OS user, with the provider account's route applied and, in a
/// managed session, its native tools withdrawn.
#[async_trait::async_trait]
pub trait CodingAgentLauncher: Send + Sync {
    /// The command the ACP driver spawns itself, for a launch the driver can start as given — the
    /// case [`Self::launch_transport`] answers with [`AgentTransport::Command`]. It cannot express
    /// an agent started elsewhere (through a supervisor), so the daemon opens conversations through
    /// [`Self::launch_transport`]; this is the driver-spawns-it half alone.
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

    /// The MCP servers the ACP session is opened with — in a managed session, `tddy-tools --mcp`
    /// ([`TDDY_TOOLS_SERVER`], [`TDDY_TOOLS_MCP_ARG`]) with the binary resolved and the agent's own
    /// tool credential minted. Asked before [`Self::launch_transport`], so a refusal here never
    /// leaves a process started.
    async fn mcp_servers_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<Vec<McpServerSpec>, SubagentError>;

    /// The command a headless coding agent's CLI runs for every turn of `launch`'s conversation: the
    /// program, the environment it adds, its working directory and any MCP config it reads. Asked
    /// only for a headless harness ([`crate::transports::transport_of`]). The session sets the
    /// model itself, so the CLI runs the model the session reports.
    ///
    /// By default refused: a launcher that starts only ACP agents has no headless command to give.
    async fn headless_command_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<HeadlessCommand, SubagentError> {
        Err(SubagentError::from(format!(
            "this launcher starts only ACP coding agents, so '{}' ({}) cannot be opened headless",
            launch.def.name, launch.def.harness
        )))
    }
}
