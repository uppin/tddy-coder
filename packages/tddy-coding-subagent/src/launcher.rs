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

/// The file in a headless conversation's directory naming the daemon that created it, by its durable
/// instance id. Two daemons can share a data directory (a desktop app beside a dev daemon), so each
/// sweeps only what it created.
pub const HEADLESS_OWNER_FILE: &str = "daemon-instance";

/// Create `dir` for a headless conversation of the daemon `daemon_instance_id`, tagged with that
/// daemon, before anything else is written into it.
pub fn claim_headless_conversation_dir(
    dir: &Path,
    daemon_instance_id: &str,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(HEADLESS_OWNER_FILE), daemon_instance_id)
}

/// The file in a headless conversation's directory listing, one absolute path per line, every file
/// its plan wrote **outside** that directory — a worktree's `.cursor/mcp.json` — so the close and
/// the start-up sweep remove those too (`#agent-tools 9/12`;
/// `docs/dev/todo/2026-10-25-cursor-project-mcp-config-outlives-its-conversation.md`).
pub const HEADLESS_WRITTEN_FILES: &str = "written-files";

/// Record `files` — a plan's `RenderedSlots::written` — in `dir`'s [`HEADLESS_WRITTEN_FILES`],
/// after what an earlier plan of the conversation recorded. Written before the files themselves,
/// so a plan that fails part-way leaves nothing the record does not name.
pub fn record_written_files(dir: &Path, files: &[PathBuf]) -> std::io::Result<()> {
    // TODO(headless-others): implement.
    let _ = (dir, files);
    todo!("headless-others: record_written_files")
}

/// Remove every file `dir`'s [`HEADLESS_WRITTEN_FILES`] records. A recorded file already gone is
/// not an error; a missing record removes nothing. Returns the files removed, in recorded order.
pub fn remove_recorded_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    // TODO(headless-others): implement.
    let _ = dir;
    todo!("headless-others: remove_recorded_files")
}

/// Close headless conversation `conversation_id` under `tddy_data_dir`: the files its record names
/// ([`remove_recorded_files`]), then its directory. What `AgentRoster::conversation_closed` does
/// for a closed conversation. A conversation with no directory is not an error. Returns the
/// recorded files removed.
pub fn close_headless_conversation(
    tddy_data_dir: &Path,
    conversation_id: &str,
) -> std::io::Result<Vec<PathBuf>> {
    // TODO(headless-others): implement — and `AgentRoster::conversation_closed`
    // (`tddy-session-agents` `coding_agent.rs`) calls it in place of its bare `remove_dir_all`.
    let _ = (tddy_data_dir, conversation_id);
    todo!("headless-others: close_headless_conversation")
}

/// Remove the files every headless conversation of the daemon `daemon_instance_id` left under
/// `<tddy_data_dir>/headless/` — what that daemon left behind when it stopped without closing them.
/// Called once at start-up, when the daemon holds no conversation (its conversation table is in
/// memory), so none of them is still in use. Another daemon's conversations, and a directory that
/// names no daemon, are left alone. A missing `headless/` is not an error. Returns the conversations
/// whose files were removed.
pub fn sweep_headless_dirs(
    tddy_data_dir: &Path,
    daemon_instance_id: &str,
) -> std::io::Result<Vec<String>> {
    let entries = match std::fs::read_dir(tddy_data_dir.join(HEADLESS_DIR)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut swept = Vec::new();
    for entry in entries {
        let dir = entry?.path();
        let owner = match std::fs::read_to_string(dir.join(HEADLESS_OWNER_FILE)) {
            Ok(owner) => owner,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        if owner.trim() != daemon_instance_id {
            continue;
        }
        // TODO(headless-others): `remove_recorded_files(&dir)?` first, so a worktree file the
        // conversation's plan wrote (`.cursor/mcp.json`) goes with it.
        std::fs::remove_dir_all(&dir)?;
        if let Some(conversation) = dir.file_name().and_then(|name| name.to_str()) {
            swept.push(conversation.to_string());
        }
    }
    Ok(swept)
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
