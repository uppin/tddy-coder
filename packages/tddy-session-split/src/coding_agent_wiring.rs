//! How a coding subagent of a managed session reaches the session's exec tools: the `tddy-tools
//! --mcp` server it is handed, with the transport and the credential that server needs.
//!
//! The credential is minted for the agent (see [`crate::agent_credentials::mint_agent_session_token`]),
//! never the caller's own token, which expires minutes into a session that runs for hours. A daemon
//! that signs no tokens refuses, rather than handing the agent a token that dies early.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon_auth::SessionTokens;
use tddy_daemon_kernel::agent_tool_sockets::AgentToolSockets;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Status;

use crate::agent_credentials::mint_agent_session_token;
use crate::split_session::colocated_jail_tool_env;
use crate::svc_resolve_tddy_tools_path::resolve_tddy_tools_path;

/// What of this daemon a coding agent's `tddy-tools --mcp` server is wired to: its config (the
/// `tddy-tools` binary, its hook URL), the agent-tool sockets it serves each session's OS user over
/// (bound under its data directory) and the tokens it mints the agent's credential with — `None` for
/// a daemon that signs none.
#[derive(Clone)]
pub struct CodingAgentToolHost {
    pub config: DaemonConfig,
    pub tddy_data_dir: PathBuf,
    pub agent_tool_sockets: Arc<AgentToolSockets>,
    pub tokens: Option<SessionTokens>,
}

impl CodingAgentToolHost {
    /// The `tddy-tools` binary and the `TDDY_REMOTE_*` environment a coding agent's MCP server runs
    /// with to serve `session_id`'s exec tools through this daemon, authenticated by a credential
    /// minted from `caller_token`, reaching this daemon over `os_user`'s agent-tool socket. A user
    /// this daemon cannot give one — under the supervisor, one not listed under
    /// `agent_tool_sockets:` — is refused, naming the setting.
    pub fn tool_server(
        &self,
        session_id: &str,
        os_user: &str,
        caller_token: &str,
    ) -> Result<(PathBuf, Vec<(String, String)>), Status> {
        let tokens = self.tokens.as_ref().ok_or_else(|| {
            Status::failed_precondition(
                "this daemon signs no session tokens, so a coding agent's tool calls could not be \
                 authenticated: configure `github:` — which gives the daemon its signing identity",
            )
        })?;
        let tddy_tools = resolve_tddy_tools_path(&self.config)?;
        let agent_tool_socket = self
            .agent_tool_sockets
            .ensure_bound_blocking(os_user, &self.tddy_data_dir)
            .map_err(|e| Status::failed_precondition(format!("{e:#}")))?;
        let env = colocated_jail_tool_env(
            &tddy_daemon_kernel::daemon_hook_urls::local_daemon_hook_url(&self.config),
            session_id,
            &mint_agent_session_token(tokens, caller_token, session_id)?,
            Some(agent_tool_socket.to_string_lossy().into_owned()),
        );
        Ok((tddy_tools, env.env_pairs()))
    }
}
