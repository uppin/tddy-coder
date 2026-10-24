//! How a coding subagent of a managed session reaches the session's exec tools: the `tddy-tools
//! --mcp` server it is handed, with the transport and the credential that server needs.
//!
//! The credential is minted for the agent (see [`crate::agent_credentials::mint_agent_session_token`]),
//! never the caller's own token, which expires minutes into a session that runs for hours. A daemon
//! that signs no tokens refuses, rather than handing the agent a token that dies early.

use std::path::Path;

use tddy_daemon_auth::SessionTokens;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Status;

use crate::agent_credentials::mint_agent_session_token;
use crate::split_session::colocated_jail_tool_env;
use crate::svc_resolve_tddy_tools_path::resolve_tddy_tools_path;

/// The `tddy-tools` binary and the `TDDY_REMOTE_*` environment a coding agent's MCP server runs
/// with to serve `session_id`'s exec tools through this daemon.
pub fn coding_agent_tool_server(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    tokens: Option<&SessionTokens>,
    session_id: &str,
    caller_token: &str,
) -> Result<(std::path::PathBuf, Vec<(String, String)>), Status> {
    let tokens = tokens.ok_or_else(|| {
        Status::failed_precondition(
            "this daemon signs no session tokens, so a coding agent's tool calls could not be \
             authenticated: configure `github:` — which gives the daemon its signing identity",
        )
    })?;
    let tddy_tools = resolve_tddy_tools_path(config)?;
    let env = colocated_jail_tool_env(
        &tddy_daemon_kernel::daemon_hook_urls::local_daemon_hook_url(config),
        session_id,
        &mint_agent_session_token(tokens, caller_token)?,
        tddy_daemon_kernel::embedded_host_agent_tool_socket(tddy_data_dir),
    );
    Ok((tddy_tools, env.env_pairs()))
}
