//! Pure builder for a session's Claude Code hooks configuration.
//!
//! The daemon calls [`build_claude_hooks_settings`] on every claude-cli launch — start and resume —
//! to produce the JSON written to the session's own hooks file
//! ([`session_file::claude_session_hooks_file`]) and loaded with `--settings`. Each of the relevant hook events is
//! wired to `tddy-tools session-hook` with the session id, daemon URL, os_user, and hook_token
//! baked in so the hook can authenticate and report status without additional config.
//!
//! All functions in this module are pure (no I/O) to maximize unit testability.

pub mod session_file;

/// Wrap `arg` so the shell that runs a hook command sees it as a single word.
///
/// A hook `command` is a **shell string**, not an argv: Claude Code and Cursor both hand it to
/// `/bin/sh -c`. The desktop install puts `tddy-tools` inside `Tddy Desktop.app`, so an unquoted
/// path splits at the space and the hook dies with
/// `/bin/sh: /Users/<user>/Applications/Tddy: No such file or directory` — the session then reports
/// no status at all, and nothing but the hook's own stderr says why.
///
/// Single quotes rather than backslashes: inside them the shell expands nothing, so a path
/// containing `$`, backticks or spaces is passed through verbatim. An embedded single quote is
/// closed, escaped and reopened (`'\''`), the one sequence single quoting cannot contain.
#[must_use]
pub fn shell_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

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
    /// The daemon's agent-facing Unix socket, set when the daemon is embedded in an application
    /// (Tddy Desktop) and so serves no HTTP RPC: there `daemon_url` is only where a GitHub sign-in
    /// returns, and every report POSTed to it is answered with a 404 the hook swallows. When set,
    /// the hook reports over this socket instead of `daemon_url`.
    pub daemon_socket: Option<&'a str>,
    /// How long an orchestrated session's `PermissionRequest` and `AskUserQuestion` hooks wait for
    /// their orchestrator's answer, in seconds (`claude_cli.answer_wait_secs`). The hook's own bound
    /// is this plus [`AWAIT_CLIENT_MARGIN_SECS`], and Claude's `timeout` for it this plus
    /// [`CLAUDE_HOOK_TIMEOUT_MARGIN_SECS`] (`#home-inbox` 6/8, 8/9).
    pub answer_wait_secs: u64,
    /// The hook carrier. `None` is the `tddy-tools session-hook` command every field above
    /// describes. `Some(log)` is a session whose agent runs on a **target**, a machine with no
    /// tddy software on it: every hook is
    /// [`remote_hook_command`](crate::remote_hook_command::remote_hook_command), which appends one
    /// line to the file `log` there, no hook that waits for an answer is installed, and no daemon
    /// socket is needed (`#remote-shell` 6/7).
    pub remote_log: Option<&'a str>,
    /// The provider route's non-secret environment pairs, pinned beside the hooks under `env` in
    /// the settings file: a tool that writes `env.ANTHROPIC_BASE_URL` into the user-level
    /// `~/.claude/settings.json` would otherwise hijack the route of every routed session, whose
    /// delivery in the child's environment a settings value replaces. The key is never here — it
    /// reaches only the agent child's environment. Empty for an unrouted launch.
    pub route_env: &'a [(String, String)],
}

/// How far the hook's own client bound sits past the daemon's deadline: the daemon answers
/// `TIMED_OUT` first.
pub const AWAIT_CLIENT_MARGIN_SECS: u64 = 5;

/// How far Claude's `timeout` for an awaiting hook sits past the daemon's deadline: a hook still
/// waiting is never killed by Claude before the daemon has answered.
pub const CLAUDE_HOOK_TIMEOUT_MARGIN_SECS: u64 = 10;

/// The `--daemon-socket` argument of a hook command, with its leading space — or nothing when the
/// daemon is reached over HTTP. Quoted like the tools path: the command is a shell string.
pub(crate) fn daemon_socket_arg(p: &HookCommandParams<'_>) -> String {
    p.daemon_socket
        .map(|socket| format!(" --daemon-socket {}", shell_quote(socket)))
        .unwrap_or_default()
}

/// The `(event, matcher)` pairs [`build_claude_hooks_settings`] installs.
const CLAUDE_HOOK_EVENTS: [(&str, &str); 9] = [
    ("SessionStart", ""),
    ("UserPromptSubmit", ""),
    ("PostToolUse", ""),
    ("Notification", ""),
    ("PermissionRequest", ""),
    ("PreToolUse", ""),
    ("PreToolUse", "AskUserQuestion"),
    ("Stop", ""),
    ("SessionEnd", ""),
];

/// The hook events [`build_claude_hooks_settings`] installs, by name.
pub const CLAUDE_INSTALLED_EVENTS: &[&str] = &event_names(CLAUDE_HOOK_EVENTS);

/// The event half of each `(event, matcher)` pair, in order.
pub(crate) const fn event_names<const N: usize>(
    events: [(&'static str, &'static str); N],
) -> [&'static str; N] {
    let mut names = [""; N];
    let mut i = 0;
    while i < N {
        names[i] = events[i].0;
        i += 1;
    }
    names
}

/// One `[{"matcher", "hooks": [...]}}]` entry for `event`: the `tddy-tools session-hook` command
/// with the session's identity baked in, awaiting the orchestrator's answer when
/// `awaits_an_answer`. Shared by the Claude settings builder and the Muse `-c hooks=` builder —
/// the two agents take the same hook commands.
pub(crate) fn hook_entry_for_event(
    p: &HookCommandParams<'_>,
    event: &str,
    matcher: &str,
    awaits_an_answer: bool,
) -> serde_json::Value {
    if let Some(log) = p.remote_log {
        let cmd = crate::remote_hook_command::remote_hook_command(event, p.hook_token, log);
        let hook = serde_json::json!({"type": "command", "command": cmd});
        return serde_json::json!([{
            "matcher": matcher,
            "hooks": [hook]
        }]);
    }
    let cmd = format!(
        "{} session-hook --session {} --daemon {}{} --os-user {} --hook-token {} --event {}",
        shell_quote(p.tddy_tools_path),
        p.session_id,
        p.daemon_url,
        daemon_socket_arg(p),
        p.os_user,
        p.hook_token,
        event,
    );
    let hook = if awaits_an_answer {
        serde_json::json!({
            "type": "command",
            "command": format!(
                "{cmd} --await-answer --await-timeout-secs {}",
                p.answer_wait_secs + AWAIT_CLIENT_MARGIN_SECS
            ),
            "timeout": p.answer_wait_secs + CLAUDE_HOOK_TIMEOUT_MARGIN_SECS,
        })
    } else {
        serde_json::json!({"type": "command", "command": cmd})
    };
    serde_json::json!([{
        "matcher": matcher,
        "hooks": [hook]
    }])
}

/// The entry of a hook that waits for an answer on a target: the POSIX waiting command, answered
/// through `<log's directory>/answers`, with Claude's `timeout` for it.
fn remote_permission_entry(
    p: &HookCommandParams<'_>,
    event: &str,
    matcher: &str,
    log: &str,
) -> serde_json::Value {
    let answers_dir = std::path::Path::new(log)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""))
        .join("answers");
    let cmd = crate::remote_hook_command::remote_answering_hook_command(
        event,
        p.hook_token,
        log,
        &answers_dir.to_string_lossy(),
        p.answer_wait_secs + AWAIT_CLIENT_MARGIN_SECS,
    );
    serde_json::json!([{
        "matcher": matcher,
        "hooks": [{
            "type": "command",
            "command": cmd,
            "timeout": p.answer_wait_secs + CLAUDE_HOOK_TIMEOUT_MARGIN_SECS,
        }]
    }])
}

/// Build a claude-cli session's hooks settings value.
///
/// Returns a `serde_json::Value` with a `hooks` object containing one entry per relevant
/// Claude Code hook event and, for a routed launch, the route's non-secret variables under `env`.
/// The caller writes this to the session's own hooks file
/// ([`session_file::claude_session_hooks_file`]).
///
/// # Format
/// Each event maps to:
/// ```json
/// { "matcher": "", "hooks": [{ "type": "command", "command": "<cmd>" }] }
/// ```
/// `matcher = ""` matches all tool names (for `PostToolUse`/`Notification`) and all other
/// variants. `PreToolUse` has two groups: `""` reports every tool call before it runs (activity
/// only, never waiting), and `AskUserQuestion` waits for the orchestrator's answer, since only that
/// tool's input is a question (`#home-inbox` 3/8, `#target-parity` 4/7).
pub fn build_claude_hooks_settings(p: &HookCommandParams<'_>) -> serde_json::Value {
    let events = CLAUDE_HOOK_EVENTS;

    let mut hooks_obj = serde_json::Map::new();
    for (event, matcher) in &events {
        // A permission prompt, and a question (`PreToolUse` matched to `AskUserQuestion`), wait
        // for the orchestrator's answer; the daemon decides whether anyone is there.
        let awaits_an_answer = *event == "PermissionRequest"
            || (*event == "PreToolUse" && *matcher == "AskUserQuestion");
        let entry = match p.remote_log {
            // On a target a waiting hook (permission, question) waits for an answer file beside
            // the hook log.
            Some(log) if awaits_an_answer => remote_permission_entry(p, event, matcher, log),
            _ => hook_entry_for_event(p, event, matcher, awaits_an_answer),
        };
        // An event's matcher groups accumulate in order: `PreToolUse` has the catch-all reporting
        // group, then the waiting `AskUserQuestion` one.
        let groups = hooks_obj
            .entry(event.to_string())
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        if let (Some(groups), Some(entry)) = (groups.as_array_mut(), entry.as_array()) {
            groups.extend(entry.iter().cloned());
        }
    }

    let mut settings = serde_json::json!({ "hooks": hooks_obj });
    if !p.route_env.is_empty() {
        let mut env = serde_json::Map::new();
        for (name, value) in p.route_env {
            env.insert(name.clone(), serde_json::Value::from(value.as_str()));
        }
        settings["env"] = env.into();
    }
    settings
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
            remote_log: None,
            route_env: &[],
        }
    }

    /// A routed launch's non-secret route variables are pinned under `env` beside the hooks, and
    /// the key is not among them — it reaches only the agent child's environment.
    #[test]
    fn build_claude_hooks_settings_pins_the_route_env_but_not_a_secret() {
        // Given
        let params = HookCommandParams {
            route_env: &[
                (
                    "ANTHROPIC_BASE_URL".to_string(),
                    "https://route.example".to_string(),
                ),
                ("ANTHROPIC_MODEL".to_string(), "routed-model".to_string()),
            ],
            ..test_params()
        };

        // When
        let value = build_claude_hooks_settings(&params);

        // Then
        assert_eq!(
            (
                value["env"]["ANTHROPIC_BASE_URL"].as_str(),
                value["env"]["ANTHROPIC_MODEL"].as_str(),
                value["env"].get("ANTHROPIC_AUTH_TOKEN"),
                value["hooks"].is_object(),
            ),
            (
                Some("https://route.example"),
                Some("routed-model"),
                None,
                true
            ),
        );
    }

    /// An unrouted launch writes no `env` key: the settings file carries the hooks alone, as
    /// before.
    #[test]
    fn build_claude_hooks_settings_writes_no_env_key_without_a_route() {
        // When
        let value = build_claude_hooks_settings(&test_params());

        // Then
        assert_eq!(value.get("env"), None);
    }

    /// The generated settings must contain all eight hook event keys.
    #[test]
    fn build_claude_hooks_settings_emits_all_eight_events() {
        // When
        let value = build_claude_hooks_settings(&test_params());
        let hooks = value
            .get("hooks")
            .expect("settings must have a 'hooks' key")
            .as_object()
            .expect("hooks must be an object");

        // Then
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

    /// The matcher groups of each installed event, in order: `PreToolUse` reports every tool
    /// (`""`), and its `AskUserQuestion` group waits for the orchestrator's answer
    /// (`#target-parity` 4/7). Every other event has one group matching all.
    fn matcher_groups(
        value: &serde_json::Value,
    ) -> std::collections::BTreeMap<String, Vec<String>> {
        value["hooks"]
            .as_object()
            .expect("hooks must be an object")
            .iter()
            .map(|(event, groups)| {
                (
                    event.clone(),
                    groups
                        .as_array()
                        .expect("an event's hooks are an array of matcher groups")
                        .iter()
                        .map(|group| {
                            group["matcher"]
                                .as_str()
                                .expect("a string matcher")
                                .to_string()
                        })
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn pre_tool_use_reports_every_tool_and_waits_on_ask_user_question() {
        // When
        let value = build_claude_hooks_settings(&test_params());

        // Then
        let one = |m: &str| vec![m.to_string()];
        assert_eq!(
            matcher_groups(&value),
            std::collections::BTreeMap::from([
                ("Notification".to_string(), one("")),
                ("PermissionRequest".to_string(), one("")),
                ("PostToolUse".to_string(), one("")),
                (
                    "PreToolUse".to_string(),
                    vec!["".to_string(), "AskUserQuestion".to_string()]
                ),
                ("SessionEnd".to_string(), one("")),
                ("SessionStart".to_string(), one("")),
                ("Stop".to_string(), one("")),
                ("UserPromptSubmit".to_string(), one("")),
            ])
        );
    }

    /// The catch-all `PreToolUse` group reports and never waits: it is the plain hook command, with
    /// no `--await-answer` and no `timeout`.
    #[test]
    fn the_catch_all_pre_tool_use_hook_only_reports() {
        // When
        let value = build_claude_hooks_settings(&test_params());

        // Then
        let catch_all = &value["hooks"]["PreToolUse"][0];
        assert_eq!(
            (
                catch_all["matcher"].as_str(),
                catch_all["hooks"][0]["command"]
                    .as_str()
                    .map(|c| c.contains("--event PreToolUse") && !c.contains("--await-answer")),
                catch_all["hooks"][0].get("timeout").is_none(),
            ),
            (Some(""), Some(true), true)
        );
    }

    /// On a target the catch-all `PreToolUse` group is the log one-liner, like every reporting hook.
    #[test]
    fn a_target_reports_every_tool_before_it_runs_through_its_hook_log() {
        // Given
        let params = HookCommandParams {
            remote_log: Some(".tddy/hooks.log"),
            ..test_params()
        };

        // When
        let value = build_claude_hooks_settings(&params);

        // Then
        assert_eq!(
            (
                value["hooks"]["PreToolUse"][0]["matcher"].as_str(),
                value["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str(),
            ),
            (
                Some(""),
                Some(
                    crate::remote_hook_command::remote_hook_command(
                        "PreToolUse",
                        "tok-xyz",
                        ".tddy/hooks.log"
                    )
                    .as_str()
                )
            )
        );
    }

    /// Each matcher group carries one command hook.
    #[test]
    fn each_matcher_group_has_one_command_hook() {
        // When
        let value = build_claude_hooks_settings(&test_params());

        // Then
        let hooks = value["hooks"].as_object().unwrap();
        for (event_name, groups) in hooks {
            for group in groups
                .as_array()
                .unwrap_or_else(|| panic!("{event_name}: hook value must be an array"))
            {
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
    }

    /// Each hook command must embed the key identifying fields and the event name.
    #[test]
    fn hook_command_includes_session_token_daemon_event_and_os_user() {
        // When
        let value = build_claude_hooks_settings(&test_params());

        // Then
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
        // When
        let value = build_claude_hooks_settings(&test_params());

        // Then
        let hooks = value["hooks"].as_object().unwrap();

        for (event_name, entries) in hooks {
            let cmd = entries[0]["hooks"][0]["command"].as_str().unwrap();
            assert!(
                cmd.starts_with("'/usr/local/bin/tddy-tools' "),
                "{event_name}: command must start with the quoted tools path; got: {cmd}"
            );
        }
    }

    /// A hook `command` is handed to `/bin/sh -c`, so a tools path containing a space must be
    /// quoted. The desktop install puts `tddy-tools` inside `Tddy Desktop.app`, and an unquoted
    /// path there fails with `/bin/sh: /…/Tddy: No such file or directory` — a session that
    /// silently reports no status.
    #[test]
    fn hook_command_quotes_a_tools_path_containing_a_space() {
        // Given
        let params = HookCommandParams {
            tddy_tools_path: "/Users/dev/Applications/Tddy Desktop.app/Contents/MacOS/tddy-tools",
            ..test_params()
        };

        // When
        let value = build_claude_hooks_settings(&params);

        // Then
        let hooks = value["hooks"].as_object().unwrap();
        for (event_name, entries) in hooks {
            let cmd = entries[0]["hooks"][0]["command"].as_str().unwrap();
            assert!(
                cmd.starts_with(
                    "'/Users/dev/Applications/Tddy Desktop.app/Contents/MacOS/tddy-tools' "
                ),
                "{event_name}: a tools path with a space must be shell-quoted; got: {cmd}"
            );
        }
    }

    /// The quoting must survive the shell it is written for: running the generated command under
    /// `/bin/sh -c` must reach the binary at the spaced path, not a prefix of it.
    #[cfg(unix)]
    #[test]
    fn generated_command_runs_under_sh_from_a_directory_with_a_space() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        // Given a `tddy-tools` inside a directory whose name contains a space
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("Tddy Desktop.app");
        std::fs::create_dir_all(&dir).expect("create dir");
        let tools = dir.join("tddy-tools");
        let mut f = std::fs::File::create(&tools).expect("create stub");
        writeln!(f, "#!/bin/sh\necho \"$1\"").expect("write stub");
        drop(f);
        std::fs::set_permissions(&tools, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        let tools_path = tools.to_string_lossy().into_owned();
        let value = build_claude_hooks_settings(&HookCommandParams {
            tddy_tools_path: &tools_path,
            ..test_params()
        });
        let cmd = value["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();

        // When
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .expect("run hook command");

        // Then
        assert!(
            out.status.success(),
            "hook command must run under sh; stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "session-hook",
            "the stub must receive 'session-hook' as its first argument"
        );
    }

    /// An embedded daemon (Tddy Desktop) serves no HTTP RPC, so a hook given its agent-tool socket
    /// must name it — shell-quoted, since the command is a shell string like the tools path.
    #[test]
    fn hook_command_names_the_daemon_socket_when_one_is_given() {
        // Given
        let params = HookCommandParams {
            daemon_socket: Some("/var/folders/x y/tddy-agent-tools-0123.sock"),
            ..test_params()
        };

        // When
        let value = build_claude_hooks_settings(&params);

        // Then
        let hooks = value["hooks"].as_object().unwrap();
        for (event_name, entries) in hooks {
            let cmd = entries[0]["hooks"][0]["command"].as_str().unwrap();
            assert!(
                cmd.contains(" --daemon-socket '/var/folders/x y/tddy-agent-tools-0123.sock' "),
                "{event_name}: the hook must report over the quoted daemon socket; got: {cmd}"
            );
        }
    }

    /// A single quote in the path must not end the quoting early.
    #[test]
    fn hook_command_escapes_a_single_quote_in_the_tools_path() {
        // Given
        let params = HookCommandParams {
            tddy_tools_path: "/home/o'brien/bin/tddy-tools",
            ..test_params()
        };

        // When
        let cmd = build_claude_hooks_settings(&params)["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .to_string();

        // Then
        assert!(
            cmd.starts_with(r"'/home/o'\''brien/bin/tddy-tools' "),
            "a single quote must be escaped for sh; got: {cmd}"
        );
    }

    /// On a target the `AskUserQuestion` group of `PreToolUse` waits for its answer in the answer
    /// file beside the hook log, as the permission hook does (`#target-parity` 6/7).
    #[test]
    fn a_target_waits_for_a_question_answer_beside_its_hook_log() {
        // Given
        let params = HookCommandParams {
            remote_log: Some(".tddy/hooks.log"),
            ..test_params()
        };

        // When
        let value = build_claude_hooks_settings(&params);

        // Then
        let question = &value["hooks"]["PreToolUse"][1];
        assert_eq!(
            (
                question["matcher"].as_str(),
                question["hooks"][0]["command"].as_str().map(str::to_string),
                question["hooks"][0]["timeout"].as_u64(),
            ),
            (
                Some("AskUserQuestion"),
                Some(crate::remote_hook_command::remote_answering_hook_command(
                    "PreToolUse",
                    "tok-xyz",
                    ".tddy/hooks.log",
                    ".tddy/answers",
                    120 + AWAIT_CLIENT_MARGIN_SECS,
                )),
                Some(120 + CLAUDE_HOOK_TIMEOUT_MARGIN_SECS),
            )
        );
    }
}
