//! Pure builder for the per-worktree `.claude/settings.local.json` hooks configuration.
//!
//! The daemon calls [`build_claude_hooks_settings`] during `start_claude_cli_session` to produce
//! the JSON that Claude Code will read from the worktree. Each of the six relevant hook events is
//! wired to `tddy-tools session-hook` with the session id, daemon URL, os_user, and hook_token
//! baked in so the hook can authenticate and report status without additional config.
//!
//! All functions in this module are pure (no I/O) to maximize unit testability.

/// Parameters for generating the per-worktree hook command strings.
pub struct HookCommandParams<'a> {
    /// Absolute path to the `tddy-tools` binary (baked in at session-start time).
    pub tddy_tools_path: &'a str,
    /// Daemon HTTP base URL for the Connect-protocol `ReportSessionStatus` RPC
    /// (e.g. `http://127.0.0.1:8899`).
    pub daemon_url: &'a str,
    /// Session id — equals the daemon session id, which Claude Code also receives via
    /// `--session-id` and reports back in hook stdin JSON.
    pub session_id: &'a str,
    /// OS user owning the session directory. Baked into the hook so the daemon can resolve
    /// `sessions_base` without a web session token.
    pub os_user: &'a str,
    /// Per-session random opaque token persisted in `.session.yaml` (`hook_token`). The daemon
    /// validates this on every `ReportSessionStatus` call to prevent cross-session spoofing.
    pub hook_token: &'a str,
}

/// Build the `.claude/settings.local.json` value for a claude-cli worktree.
///
/// Returns a `serde_json::Value` with a `hooks` object containing one entry per relevant
/// Claude Code hook event. The caller writes this to `<worktree>/.claude/settings.local.json`.
///
/// # Format
/// Each event maps to:
/// ```json
/// { "matcher": "", "hooks": [{ "type": "command", "command": "<cmd>" }] }
/// ```
/// `matcher = ""` matches all tool names (for `PostToolUse`/`Notification`) and all other
/// variants.
pub fn build_claude_hooks_settings(p: &HookCommandParams<'_>) -> serde_json::Value {
    let events = [
        "SessionStart",
        "UserPromptSubmit",
        "PostToolUse",
        "Notification",
        "Stop",
        "SessionEnd",
    ];

    let mut hooks_obj = serde_json::Map::new();
    for event in &events {
        let cmd = format!(
            "{} session-hook --session {} --daemon {} --os-user {} --hook-token {} --event {}",
            p.tddy_tools_path, p.session_id, p.daemon_url, p.os_user, p.hook_token, event,
        );
        let hook_entry = serde_json::json!([{
            "matcher": "",
            "hooks": [{"type": "command", "command": cmd}]
        }]);
        hooks_obj.insert(event.to_string(), hook_entry);
    }

    serde_json::json!({ "hooks": hooks_obj })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_params() -> HookCommandParams<'static> {
        HookCommandParams {
            tddy_tools_path: "/usr/local/bin/tddy-tools",
            daemon_url: "http://127.0.0.1:8899",
            session_id: "sess-abc123",
            os_user: "alice",
            hook_token: "tok-xyz",
        }
    }

    /// The generated settings must contain all six hook event keys.
    #[test]
    fn build_claude_hooks_settings_emits_all_six_events() {
        let value = build_claude_hooks_settings(&test_params());
        let hooks = value
            .get("hooks")
            .expect("settings must have a 'hooks' key")
            .as_object()
            .expect("hooks must be an object");

        for event in &[
            "SessionStart",
            "UserPromptSubmit",
            "PostToolUse",
            "Notification",
            "Stop",
            "SessionEnd",
        ] {
            assert!(
                hooks.contains_key(*event),
                "hooks must contain event '{event}'"
            );
        }
        assert_eq!(hooks.len(), 6, "hooks must contain exactly 6 events");
    }

    /// Each hook entry must be an array with one matcher group containing one command hook.
    #[test]
    fn each_event_has_one_matcher_group_with_one_command_hook() {
        let value = build_claude_hooks_settings(&test_params());
        let hooks = value["hooks"].as_object().unwrap();

        for (event_name, entries) in hooks {
            let arr = entries
                .as_array()
                .unwrap_or_else(|| panic!("{event_name}: hook value must be an array"));
            assert_eq!(
                arr.len(),
                1,
                "{event_name}: must have exactly one matcher group"
            );

            let group = &arr[0];
            assert_eq!(
                group.get("matcher").and_then(|v| v.as_str()),
                Some(""),
                "{event_name}: matcher must be empty string"
            );

            let inner = group["hooks"]
                .as_array()
                .unwrap_or_else(|| panic!("{event_name}: inner hooks must be array"));
            assert_eq!(inner.len(), 1, "{event_name}: must have exactly one hook");
            assert_eq!(
                inner[0].get("type").and_then(|v| v.as_str()),
                Some("command"),
                "{event_name}: hook type must be 'command'"
            );
        }
    }

    /// Each hook command must embed the key identifying fields and the event name.
    #[test]
    fn hook_command_includes_session_token_daemon_event_and_os_user() {
        let value = build_claude_hooks_settings(&test_params());
        let hooks = value["hooks"].as_object().unwrap();

        for (event_name, entries) in hooks {
            let cmd = entries[0]["hooks"][0]["command"]
                .as_str()
                .unwrap_or_else(|| panic!("{event_name}: command must be a string"));

            assert!(
                cmd.contains("session-hook"),
                "{event_name}: command must contain 'session-hook'; got: {cmd}"
            );
            assert!(
                cmd.contains("--session sess-abc123"),
                "{event_name}: command must contain '--session sess-abc123'; got: {cmd}"
            );
            assert!(
                cmd.contains("--daemon http://127.0.0.1:8899"),
                "{event_name}: command must contain '--daemon http://127.0.0.1:8899'; got: {cmd}"
            );
            assert!(
                cmd.contains("--os-user alice"),
                "{event_name}: command must contain '--os-user alice'; got: {cmd}"
            );
            assert!(
                cmd.contains("--hook-token tok-xyz"),
                "{event_name}: command must contain '--hook-token tok-xyz'; got: {cmd}"
            );
            assert!(
                cmd.contains(&format!("--event {event_name}")),
                "{event_name}: command must contain '--event {event_name}'; got: {cmd}"
            );
        }
    }

    /// The hook command must start with the configured `tddy_tools_path`.
    #[test]
    fn hook_command_uses_provided_tddy_tools_path() {
        let value = build_claude_hooks_settings(&test_params());
        let hooks = value["hooks"].as_object().unwrap();

        for (event_name, entries) in hooks {
            let cmd = entries[0]["hooks"][0]["command"].as_str().unwrap();
            assert!(
                cmd.starts_with("/usr/local/bin/tddy-tools"),
                "{event_name}: command must start with the configured tools path; got: {cmd}"
            );
        }
    }
}
