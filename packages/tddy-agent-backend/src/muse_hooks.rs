//! Pure builder for Muse Code's `-c hooks=…` lifecycle-hooks value.
//!
//! The daemon renders [`build_muse_hooks_config_value`] during a `muse-cli` start (and resume)
//! into the `muse -c hooks=<json>` argv entries: project `.muse/hooks.json` is not loaded before
//! workspace trust, while `-c` installs the same hooks without a file the agent could commit —
//! and merges with the user's `managed_hooks_path` hooks instead of replacing them
//! (`docs/dev/changesets/2026-11-15-muse-hooks-wired.md`, probe 2026-11-15).
//!
//! The events and commands are Claude's
//! ([`crate::claude_hooks::build_claude_hooks_settings`]), built by the shared
//! [`crate::claude_hooks::hook_entry_for_event`]; only the delivery differs (argv, not a settings
//! file). Every hook is
//! report-only: Muse's `PreToolUse` never fires for `request_user_input`, so no hook of ours can
//! carry a blocking answer.
//!
//! All functions in this module are pure (no I/O) to maximize unit testability.

use crate::claude_hooks::{event_names, hook_entry_for_event, HookCommandParams};

/// The `(event, matcher)` pairs [`build_muse_hooks_config_value`] installs: Claude's eight, with
/// `PreToolUse` matched to Muse's question tool.
const MUSE_HOOK_EVENTS: [(&str, &str); 8] = [
    ("SessionStart", ""),
    ("UserPromptSubmit", ""),
    ("PostToolUse", ""),
    ("Notification", ""),
    ("PermissionRequest", ""),
    ("PreToolUse", "request_user_input"),
    ("Stop", ""),
    ("SessionEnd", ""),
];

/// The hook events [`build_muse_hooks_config_value`] installs, by name.
pub const MUSE_INSTALLED_EVENTS: &[&str] = &event_names(MUSE_HOOK_EVENTS);

/// Build the `-c hooks=…` value for a `muse-cli` launch.
///
/// Returns the hooks map itself — `{"SessionStart": [...], …}` — without the file's top-level
/// `hooks` key: `-c` sets the `hooks` setting, whose value is that map. The caller serializes it
/// compactly into the `hooks=<json>` argv entry.
pub fn build_muse_hooks_config_value(p: &HookCommandParams<'_>) -> serde_json::Value {
    let mut hooks_obj = serde_json::Map::new();
    for (event, matcher) in &MUSE_HOOK_EVENTS {
        // Report-only, every event: no Muse hook of ours waits for an orchestrator's answer.
        hooks_obj.insert(
            event.to_string(),
            hook_entry_for_event(p, event, matcher, false),
        );
    }
    serde_json::Value::Object(hooks_obj)
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
            daemon_socket: None,
            answer_wait_secs: 120,
        }
    }

    /// The value is the hooks map itself — the `-c hooks=` setting's value, not a settings file —
    /// with Claude's eight events.
    #[test]
    fn build_muse_hooks_config_value_emits_all_eight_events_without_a_hooks_key() {
        // When
        let value = build_muse_hooks_config_value(&test_params());

        // Then
        let hooks = value.as_object().expect("the value must be the hooks map");
        assert!(
            !hooks.contains_key("hooks"),
            "the -c value must not nest under a top-level 'hooks' key"
        );
        let mut events: Vec<&str> = hooks.keys().map(String::as_str).collect();
        events.sort_unstable();
        assert_eq!(
            events,
            vec![
                "Notification",
                "PermissionRequest",
                "PostToolUse",
                "PreToolUse",
                "SessionEnd",
                "SessionStart",
                "Stop",
                "UserPromptSubmit",
            ]
        );
    }

    /// `PreToolUse` is installed for Muse's question tool alone; every other event matches all.
    #[test]
    fn the_pre_tool_use_hook_matches_request_user_input_only() {
        // When
        let value = build_muse_hooks_config_value(&test_params());

        // Then
        let hooks = value.as_object().expect("the value must be the hooks map");
        let matchers: std::collections::BTreeMap<&str, &str> = hooks
            .iter()
            .map(|(event, entries)| {
                (
                    event.as_str(),
                    entries[0]["matcher"].as_str().expect("a string matcher"),
                )
            })
            .collect();
        assert_eq!(
            matchers,
            std::collections::BTreeMap::from([
                ("Notification", ""),
                ("PermissionRequest", ""),
                ("PostToolUse", ""),
                ("PreToolUse", "request_user_input"),
                ("SessionEnd", ""),
                ("SessionStart", ""),
                ("Stop", ""),
                ("UserPromptSubmit", ""),
            ])
        );
    }

    /// Each hook command embeds the session's identity and its event, as Claude's do.
    #[test]
    fn hook_command_includes_session_token_daemon_event_and_os_user() {
        // When
        let value = build_muse_hooks_config_value(&test_params());

        // Then
        let hooks = value.as_object().unwrap();
        for (event_name, entries) in hooks {
            let cmd = entries[0]["hooks"][0]["command"]
                .as_str()
                .unwrap_or_else(|| panic!("{event_name}: command must be a string"));
            for expected in [
                "session-hook",
                "--session sess-abc123",
                "--daemon http://127.0.0.1:8899",
                "--os-user alice",
                "--hook-token tok-xyz",
                &format!("--event {event_name}"),
            ] {
                assert!(
                    cmd.contains(expected),
                    "{event_name}: command must contain '{expected}'; got: {cmd}"
                );
            }
            assert!(
                cmd.starts_with("'/usr/local/bin/tddy-tools' "),
                "{event_name}: command must start with the quoted tools path; got: {cmd}"
            );
        }
    }

    /// No Muse hook waits for an orchestrator's answer: `PreToolUse` never fires for
    /// `request_user_input`, so a blocking hook could never deliver one — and an `--await-answer`
    /// hook on an event Muse does not block on would only hang the hook process.
    #[test]
    fn no_hook_awaits_an_answer() {
        // When
        let value = build_muse_hooks_config_value(&test_params());

        // Then
        let hooks = value.as_object().unwrap();
        for (event_name, entries) in hooks {
            let hook = &entries[0]["hooks"][0];
            let cmd = hook["command"].as_str().unwrap();
            assert!(
                !cmd.contains("--await-answer"),
                "{event_name}: no Muse hook awaits an answer; got: {cmd}"
            );
            assert!(
                hook.get("timeout").is_none(),
                "{event_name}: a hook that awaits nothing carries no timeout"
            );
        }
    }
}
