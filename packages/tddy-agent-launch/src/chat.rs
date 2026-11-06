use tddy_core::HookCommandParams;

use tddy_core::build_cursor_hooks_settings;

use uuid::Uuid;

use std::path::Path;

use tddy_daemon_kernel::config::DaemonConfig;

/// Write `.cursor/hooks.json` under `worktree_path` for a cursor-cli session.
///
/// Returns the generated per-session hook token (also embedded in hook commands).
// TODO(cli-dispatch-collapse): kept — Telegram's launcher still calls it
// (`tddy-telegram-control/src/telegram_session_control/cli_spawns.rs`); `#agent-tools 18/19`
// `telegram-spec-launch` deletes it (docs/dev/todo/2026-11-03-cursor-hand-built-launch-paths.md).
pub fn install_cursor_hooks_in_worktree(
    config: &DaemonConfig,
    worktree_path: &Path,
    session_id: &str,
    os_user: &str,
) -> String {
    let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
        tddy_daemon_kernel::config::resolve_cursor_cli_tddy_tools_path(config).as_deref(),
    );

    let daemon_url = cursor_hook_daemon_url(config);

    let hook_token = Uuid::new_v4().to_string();
    let hooks_settings = build_cursor_hooks_settings(&HookCommandParams {
        tddy_tools_path: &tddy_tools_path,
        daemon_url: &daemon_url,
        session_id,
        os_user,
        hook_token: &hook_token,
        // The hand-built path, reporting over HTTP. No daemon path calls it: every Cursor launch
        // renders its hooks through the `cursor-hooks-json` adapter, over the daemon's agent-tool
        // socket. Deleted once approved: docs/dev/todo/2026-11-03-cursor-hand-built-launch-paths.md.
        daemon_socket: None,
        // Cursor hooks never wait for an answer; the value is unread.
        answer_wait_secs: config.claude_cli_answer_wait_secs(),
    });
    let cursor_dir = worktree_path.join(".cursor");
    if let Err(e) = std::fs::create_dir_all(&cursor_dir).and_then(|_| {
        serde_json::to_string_pretty(&hooks_settings)
            .map_err(|e| std::io::Error::other(e.to_string()))
            .and_then(|json| {
                tddy_core::atomic_file::write_atomic(&cursor_dir.join("hooks.json"), json)
            })
    }) {
        log::warn!(
            "session {session_id}: failed to write .cursor/hooks.json — hooks will not fire: {e}"
        );
    }
    hook_token
}

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

/// Mint the Cursor chat a session will own by running `<binary_path> create-chat` in `worktree_path`,
/// through the `cursor-create-chat` launch adapter, bounded by
/// [`tddy_coding_agent_specs::slots::conversation::DEFAULT_CONVERSATION_MINT_DEADLINE_SECS`].
///
/// Only `create-chat` mints real chat ids, so a failure here is returned to the caller: starting the
/// agent without one would produce a session no resume can reattach to.
// TODO(cli-dispatch-collapse): kept — Telegram's launcher still calls it
// (`tddy-telegram-control/src/telegram_session_control/cli_spawns.rs`); `#agent-tools 18/19`
// `telegram-spec-launch` deletes it (docs/dev/todo/2026-11-03-cursor-hand-built-launch-paths.md).
pub async fn mint_cursor_chat_id(
    binary_path: &str,
    worktree_path: &Path,
) -> Result<String, String> {
    use tddy_core::adapters::{ConversationIdAdapter, ConversationMint, CursorCreateChat};
    CursorCreateChat
        .mint(ConversationMint {
            binary: Path::new(binary_path),
            workdir: worktree_path,
            env: &[],
            deadline: std::time::Duration::from_secs(
                tddy_coding_agent_specs::slots::conversation::DEFAULT_CONVERSATION_MINT_DEADLINE_SECS,
            ),
        })
        .await
        .map_err(|e| e.to_string())
}
