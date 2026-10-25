//! How a coding subagent of a managed session reaches the session's exec tools: the `tddy-tools
//! --mcp` server it is handed, with the transport and the credential that server needs.
//!
//! The credential is minted for the agent (see [`crate::agent_credentials::mint_agent_session_token`]),
//! never the caller's own token, which expires minutes into a session that runs for hours. A daemon
//! that signs no tokens refuses, rather than handing the agent a token that dies early.

use std::path::PathBuf;

use tddy_daemon_auth::SessionTokens;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Status;

use crate::agent_credentials::mint_agent_session_token;
use crate::split_session::colocated_jail_tool_env;
use crate::svc_resolve_tddy_tools_path::resolve_tddy_tools_path;

/// What of this daemon a coding agent's `tddy-tools --mcp` server is wired to: its config (the
/// `tddy-tools` binary, its hook URL), its data directory (the agent tool socket) and the tokens it
/// mints the agent's credential with — `None` for a daemon that signs none.
#[derive(Clone)]
pub struct CodingAgentToolHost {
    pub config: DaemonConfig,
    pub tddy_data_dir: PathBuf,
    pub tokens: Option<SessionTokens>,
}

impl CodingAgentToolHost {
    /// The `tddy-tools` binary and the `TDDY_REMOTE_*` environment a coding agent's MCP server runs
    /// with to serve `session_id`'s exec tools through this daemon, authenticated by a credential
    /// minted from `caller_token`.
    pub fn tool_server(
        &self,
        session_id: &str,
        caller_token: &str,
    ) -> Result<(PathBuf, Vec<(String, String)>), Status> {
        let tokens = self.tokens.as_ref().ok_or_else(|| {
            Status::failed_precondition(
                "this daemon signs no session tokens, so a coding agent's tool calls could not be \
                 authenticated: configure `github:` — which gives the daemon its signing identity",
            )
        })?;
        let tddy_tools = resolve_tddy_tools_path(&self.config)?;
        let env = colocated_jail_tool_env(
            &tddy_daemon_kernel::daemon_hook_urls::local_daemon_hook_url(&self.config),
            session_id,
            &mint_agent_session_token(tokens, caller_token)?,
            tddy_daemon_kernel::embedded_host_agent_tool_socket(&self.tddy_data_dir),
        );
        Ok((tddy_tools, env.env_pairs()))
    }
}
