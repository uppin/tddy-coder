use std::path::Path;

use tddy_daemon_kernel::config::DaemonConfig;

/// The URL a cursor-cli session's hooks — and its `tddy-tools notify` — report to:
/// `cursor_cli.daemon_url`, then `claude_cli.daemon_url`, then this daemon's own web listener, the
/// same last resort every hook URL falls back to.
pub(crate) fn cursor_hook_daemon_url(config: &DaemonConfig) -> String {
    tddy_daemon_kernel::config::resolve_cursor_cli_daemon_url(config)
        .unwrap_or_else(|| tddy_daemon_kernel::daemon_hook_urls::local_daemon_hook_url(config))
}

/// The environment a cursor-cli agent is given so `tddy-tools notify` can reach its operator
/// (`#home-inbox` 1/8): its session, owner, hook token and the daemon — over the agent-tool socket
/// on an embedded daemon, which serves no HTTP RPC, and over the hooks' URL otherwise.
pub fn cursor_notify_env(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    os_user: &str,
    session_id: &str,
    hook_token: &str,
) -> Vec<(String, String)> {
    let daemon_socket = tddy_daemon_kernel::embedded_host_agent_tool_socket(tddy_data_dir);
    crate::notify_env::notify_env_pairs(
        session_id,
        os_user,
        hook_token,
        &cursor_hook_daemon_url(config),
        daemon_socket.as_deref().map(Path::new),
    )
}

/// [`cursor_notify_env`] for a resumed session, from the hook token its `.session.yaml` recorded;
/// a record with none is given nothing.
pub fn resumed_cursor_notify_env(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    os_user: &str,
    meta: &tddy_core::SessionMetadata,
) -> Vec<(String, String)> {
    meta.hook_token
        .as_deref()
        .filter(|token| !token.is_empty())
        .map(|token| cursor_notify_env(config, tddy_data_dir, os_user, &meta.session_id, token))
        .unwrap_or_default()
}

/// The chat id printed by `cursor-agent create-chat`, or a description of what came out instead.
///
pub use tddy_core::adapters::conversation_id::parse_created_chat_id;
