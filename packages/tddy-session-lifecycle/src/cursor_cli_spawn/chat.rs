use tddy_core::HookCommandParams;

use tddy_core::build_cursor_hooks_settings;

use uuid::Uuid;

use std::path::Path;

use crate::config::DaemonConfig;

/// Write `.cursor/hooks.json` under `worktree_path` for a cursor-cli session.
///
/// Returns the generated per-session hook token (also embedded in hook commands).
pub fn install_cursor_hooks_in_worktree(
    config: &DaemonConfig,
    worktree_path: &Path,
    session_id: &str,
    os_user: &str,
) -> String {
    let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
        crate::config::resolve_cursor_cli_tddy_tools_path(config).as_deref(),
    );

    // `cursor_cli.daemon_url`, then `claude_cli.daemon_url`, then this daemon's own web listener —
    // the same last resort every hook URL falls back to.
    let daemon_url = crate::config::resolve_cursor_cli_daemon_url(config)
        .unwrap_or_else(|| crate::connection_service::local_daemon_hook_url(config));

    let hook_token = Uuid::new_v4().to_string();
    let hooks_settings = build_cursor_hooks_settings(&HookCommandParams {
        tddy_tools_path: &tddy_tools_path,
        daemon_url: &daemon_url,
        session_id,
        os_user,
        hook_token: &hook_token,
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

/// The chat id printed by `cursor-agent create-chat`, or a description of what came out instead.
///
/// `create-chat` prints a **bare** chat id on stdout and exits 0 ("Create a new empty chat and
/// return its ID" — cursor-agent 2026.07.23). Anything else — no output, a banner, a prompt, an
/// error message — is not a chat id, and pinning it to the session would send every later resume
/// into a chat that does not exist.
pub fn parse_created_chat_id(stdout: &str) -> Result<String, String> {
    let chat_id = stdout.trim();
    if chat_id.is_empty() {
        return Err("printed no chat id on stdout".to_string());
    }
    if chat_id.split_whitespace().count() > 1 {
        return Err(format!(
            "printed {chat_id:?} on stdout, which is not a bare chat id"
        ));
    }
    Ok(chat_id.to_string())
}

/// Mint the Cursor chat a session will own by running `<binary_path> create-chat` in `worktree_path`.
///
/// Only `create-chat` mints real chat ids, so a failure here is returned to the caller: starting the
/// agent without one would produce a session no resume can reattach to.
pub async fn mint_cursor_chat_id(
    binary_path: &str,
    worktree_path: &Path,
) -> Result<String, String> {
    let output = tokio::process::Command::new(binary_path)
        .arg("create-chat")
        .current_dir(worktree_path)
        .output()
        .await
        .map_err(|e| format!("`{binary_path} create-chat` could not be run: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`{binary_path} create-chat` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_created_chat_id(&String::from_utf8_lossy(&output.stdout))
        .map_err(|e| format!("`{binary_path} create-chat` {e}"))
}
