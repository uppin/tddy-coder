//! Unit tests: where a session's Claude hooks live, and releasing the legacy worktree file.

use super::*;
use crate::claude_hooks::{build_claude_hooks_settings, HookCommandParams};
use pretty_assertions::assert_eq;

fn params<'a>(session_id: &'a str, remote_log: Option<&'a str>) -> HookCommandParams<'a> {
    HookCommandParams {
        tddy_tools_path: "/usr/local/bin/tddy-tools",
        daemon_url: "http://127.0.0.1:8899",
        session_id,
        os_user: "ada",
        hook_token: "tok-1",
        daemon_socket: Some("/tmp/tddy-agent-tools-46362-0.sock"),
        answer_wait_secs: 120,
        remote_log,
        route_env: &[],
    }
}

/// What the daemon used to write into `.claude/settings.local.json` for `session_id`.
fn the_daemons_legacy_settings(session_id: &str) -> serde_json::Value {
    build_claude_hooks_settings(&params(session_id, None))
}

fn released(settings: &serde_json::Value) -> LegacyRelease {
    release_legacy_claude_settings(settings.to_string().as_bytes())
}

#[test]
fn a_sessions_hooks_file_is_named_by_the_session_under_the_worktrees_tddy_hooks() {
    assert_eq!(
        claude_session_hooks_file(Path::new("/w"), "01a16dc6"),
        PathBuf::from("/w/.tddy/hooks/01a16dc6/claude-settings.json")
    );
}

#[test]
fn a_file_holding_only_the_daemons_hooks_is_removed() {
    assert_eq!(
        released(&the_daemons_legacy_settings("01a1f0a7")),
        LegacyRelease::Remove
    );
}

#[test]
fn a_targets_hook_log_appends_are_the_daemons_too() {
    let settings = build_claude_hooks_settings(&params("s1", Some(REMOTE_HOOK_LOG)));
    assert_eq!(released(&settings), LegacyRelease::Remove);
}

#[test]
fn the_route_pin_the_daemon_wrote_beside_its_hooks_goes_with_them() {
    let route = [(
        "ANTHROPIC_BASE_URL".to_string(),
        "https://route".to_string(),
    )];
    let settings = build_claude_hooks_settings(&HookCommandParams {
        route_env: &route,
        ..params("s1", None)
    });
    assert_eq!(released(&settings), LegacyRelease::Remove);
}

#[test]
fn the_operators_own_permissions_and_hooks_are_kept() {
    // Given the daemon's hooks, beside a permission rule Claude Code saved and a hook of the
    // operator's own on the same event
    let mut settings = the_daemons_legacy_settings("01a1f0a7");
    settings["permissions"] = serde_json::json!({ "allow": ["Bash(ls:*)"] });
    settings["hooks"]["Stop"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "matcher": "",
            "hooks": [{ "type": "command", "command": "say done" }]
        }));

    // When it is released
    let LegacyRelease::Rewrite(bytes) = released(&settings) else {
        panic!("the operator's entries keep the file");
    };

    // Then only the operator's entries remain
    let kept: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        kept,
        serde_json::json!({
            "permissions": { "allow": ["Bash(ls:*)"] },
            "hooks": { "Stop": [{
                "matcher": "",
                "hooks": [{ "type": "command", "command": "say done" }]
            }] }
        })
    );
}

#[test]
fn a_file_without_the_daemons_hooks_is_left_alone() {
    let settings = serde_json::json!({
        "env": { "FOO": "bar" },
        "hooks": { "Stop": [{ "matcher": "", "hooks": [{ "type": "command", "command": "say done" }] }] }
    });
    assert_eq!(released(&settings), LegacyRelease::Keep);
}

#[test]
fn a_file_that_is_not_a_json_object_is_left_alone() {
    assert_eq!(
        release_legacy_claude_settings(b"not json"),
        LegacyRelease::Keep
    );
    assert_eq!(release_legacy_claude_settings(b"[]"), LegacyRelease::Keep);
}
