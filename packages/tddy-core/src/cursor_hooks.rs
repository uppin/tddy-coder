//! Pure builder for the per-worktree `.cursor/hooks.json` configuration.
//!
//! The daemon calls [`build_cursor_hooks_settings`] during `start_cursor_cli_session` to produce
//! the JSON that Cursor Agent CLI reads from the worktree. Each of the five relevant hook events
//! is wired to `tddy-tools session-hook` with session id, daemon URL, os_user, and hook_token
//! baked in. Event names are read from stdin JSON (`hook_event_name`) by the hook binary.

use crate::claude_hooks::HookCommandParams;

/// Cursor lifecycle hooks wired for activity status reporting.
const CURSOR_HOOK_EVENTS: &[&str] = &[
    "sessionStart",
    "beforeSubmitPrompt",
    "postToolUse",
    "stop",
    "sessionEnd",
];

/// Build the `.cursor/hooks.json` value for a cursor-cli worktree.
///
/// Returns a `serde_json::Value` with `version: 1` and a `hooks` object. The caller writes this
/// to `<worktree>/.cursor/hooks.json`.
pub fn build_cursor_hooks_settings(p: &HookCommandParams<'_>) -> serde_json::Value {
    let mut hooks_obj = serde_json::Map::new();
    for event in CURSOR_HOOK_EVENTS {
        let cmd = format!(
            "{} session-hook --session {} --daemon {} --os-user {} --hook-token {}",
            p.tddy_tools_path, p.session_id, p.daemon_url, p.os_user, p.hook_token,
        );
        hooks_obj.insert(
            (*event).to_string(),
            serde_json::json!([{ "command": cmd }]),
        );
    }

    serde_json::json!({
        "version": 1,
        "hooks": hooks_obj,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_hooks::HookCommandParams;

    fn test_params() -> HookCommandParams<'static> {
        HookCommandParams {
            tddy_tools_path: "/usr/local/bin/tddy-tools",
            daemon_url: "http://127.0.0.1:8899",
            session_id: "sess-abc123",
            os_user: "alice",
            hook_token: "tok-xyz",
        }
    }

    #[test]
    fn build_cursor_hooks_settings_emits_version_and_five_events() {
        let value = build_cursor_hooks_settings(&test_params());
        assert_eq!(value.get("version").and_then(|v| v.as_i64()), Some(1));
        let hooks = value["hooks"].as_object().expect("hooks object");
        for event in CURSOR_HOOK_EVENTS {
            assert!(hooks.contains_key(*event), "missing event {event}");
        }
        assert_eq!(hooks.len(), 5);
    }

    #[test]
    fn each_cursor_hook_command_includes_session_hook_and_hook_token() {
        let value = build_cursor_hooks_settings(&test_params());
        let hooks = value["hooks"].as_object().unwrap();
        for (event_name, entries) in hooks {
            let cmd = entries[0]["command"]
                .as_str()
                .unwrap_or_else(|| panic!("{event_name}: command must be a string"));
            assert!(cmd.contains("session-hook"), "{event_name}: {cmd}");
            assert!(cmd.contains("--hook-token tok-xyz"), "{event_name}: {cmd}");
            assert!(
                !cmd.contains("--event"),
                "{event_name}: cursor hooks must not bake --event; got: {cmd}"
            );
        }
    }
}
