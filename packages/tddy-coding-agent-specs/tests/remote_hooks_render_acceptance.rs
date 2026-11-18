//! Acceptance: the hooks an agent gets when its session runs on a **target** — a machine with no
//! tddy software on it.
//!
//! Every hooks adapter renders the same command from one `HookCommandParams`. With a remote log in
//! those params the command is the log one-liner (`tddy_agent_backend::remote_hook_command`), and
//! no hook that waits for an answer is installed: nothing travels back to an agent on a target.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-remote-hooks.md.

use std::collections::BTreeMap;
use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_agent_backend::claude_hooks::HookCommandParams;
use tddy_agent_backend::remote_hook_command::remote_hook_command;
use tddy_coding_agent_specs::slots::hooks::{adapter, RenderedHooks};
use tddy_coding_agent_specs::slots::SlotError;

/// The session's worktree: on the target for a target session, on this host for a local one.
const WORKTREE: &str = "/home/dev/repo/.worktrees/019d2c7e";
/// The hook log of a target session, in its worktree on the target.
const REMOTE_LOG: &str = "/home/dev/repo/.worktrees/019d2c7e/.tddy/hooks.log";
const HOOK_TOKEN: &str = "tok-7f3a9c";
const TOOLS: &str = "/usr/local/bin/tddy-tools";
const DAEMON_SOCKET: &str = "/var/folders/xy/T/tddy-agent-tools-1a2b.sock";

/// The events Claude Code and Muse report with and wait on nothing for.
const REPORTING_EVENTS: [&str; 6] = [
    "SessionStart",
    "UserPromptSubmit",
    "PostToolUse",
    "Notification",
    "Stop",
    "SessionEnd",
];
/// The events Cursor reports with.
const CURSOR_EVENTS: [&str; 7] = [
    "sessionStart",
    "beforeSubmitPrompt",
    "preToolUse",
    "postToolUse",
    "postToolUseFailure",
    "stop",
    "sessionEnd",
];

fn a_session() -> HookCommandParams<'static> {
    HookCommandParams {
        tddy_tools_path: TOOLS,
        daemon_url: "http://127.0.0.1:8899",
        session_id: "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e10",
        os_user: "dev",
        hook_token: HOOK_TOKEN,
        daemon_socket: None,
        answer_wait_secs: 120,
        remote_log: None,
        route_env: &[],
    }
}

/// A session whose agent runs on a target: its hooks append to [`REMOTE_LOG`] there, and the
/// daemon's agent-tool socket, which the target cannot reach, is not given.
fn a_session_on_a_target() -> HookCommandParams<'static> {
    HookCommandParams {
        remote_log: Some(REMOTE_LOG),
        ..a_session()
    }
}

/// A session whose agent runs on this host, reporting over the daemon's agent-tool socket.
fn a_local_session() -> HookCommandParams<'static> {
    HookCommandParams {
        daemon_socket: Some(DAEMON_SOCKET),
        ..a_session()
    }
}

/// The log command of each of `events`, by event.
fn the_log_command_for_each_of(events: &[&str]) -> BTreeMap<String, String> {
    events
        .iter()
        .map(|event| {
            (
                event.to_string(),
                remote_hook_command(event, HOOK_TOKEN, REMOTE_LOG),
            )
        })
        .collect()
}

/// What an adapter rendered for a session, read as its agent reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InstalledHooks {
    /// Every installed hook's command, by event.
    commands: BTreeMap<String, String>,
    /// The events whose hook the agent holds until it answers: one with a `timeout`, or told to
    /// await an answer.
    waiting: Vec<String>,
    /// Whether anything rendered names the tools binary or its `session-hook` subcommand.
    names_a_tddy_binary: bool,
}

impl InstalledHooks {
    /// The commands of `events` alone.
    fn commands_of(&self, events: &[&str]) -> BTreeMap<String, String> {
        self.commands
            .iter()
            .filter(|(event, _)| events.contains(&event.as_str()))
            .map(|(event, command)| (event.clone(), command.clone()))
            .collect()
    }

    /// The installed events, sorted.
    fn events(&self) -> Vec<String> {
        self.commands.keys().cloned().collect()
    }
}

/// The hooks of Claude's settings shape — `{event: [{matcher, hooks: [{command, timeout?}]}]}` —
/// which Muse's `-c hooks=` value shares.
fn hooks_in_claude_shape(hooks: &serde_json::Value, rendered: &str) -> InstalledHooks {
    let entries: Vec<(String, serde_json::Value)> = hooks
        .as_object()
        .map(|events| {
            events
                .iter()
                .map(|(event, groups)| (event.clone(), groups[0]["hooks"][0].clone()))
                .collect()
        })
        .unwrap_or_default();
    InstalledHooks {
        commands: entries
            .iter()
            .map(|(event, hook)| {
                (
                    event.clone(),
                    hook["command"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect(),
        waiting: entries
            .iter()
            .filter(|(_, hook)| {
                hook.get("timeout").is_some()
                    || hook["command"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("--await-answer")
            })
            .map(|(event, _)| event.clone())
            .collect(),
        names_a_tddy_binary: names_a_tddy_binary(rendered),
    }
}

fn names_a_tddy_binary(rendered: &str) -> bool {
    rendered.contains("tddy-tools") || rendered.contains("session-hook")
}

/// What the adapter named `name` renders for `session`.
fn rendered_by(name: &str, session: &HookCommandParams<'_>) -> Result<RenderedHooks, SlotError> {
    adapter(name).and_then(|adapter| adapter.try_render(Path::new(WORKTREE), session))
}

/// The bytes of a rendered hooks file, as JSON and as text.
fn the_file(rendered: RenderedHooks) -> (serde_json::Value, String) {
    let text = match rendered {
        RenderedHooks::File(_, bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        RenderedHooks::Argv(argv) => panic!("a file adapter rendered argv: {argv:?}"),
    };
    (
        serde_json::from_str(&text).expect("a hooks file is JSON"),
        text,
    )
}

/// The hooks Claude Code reads from `.claude/settings.local.json` for `session`.
fn claude_hooks_for(session: &HookCommandParams<'_>) -> Result<InstalledHooks, SlotError> {
    rendered_by("claude-settings-local", session).map(|rendered| {
        let (settings, text) = the_file(rendered);
        hooks_in_claude_shape(&settings["hooks"], &text)
    })
}

/// The hooks Cursor reads from `.cursor/hooks.json` for `session`: `{hooks: {event: [{command}]}}`.
fn cursor_hooks_for(session: &HookCommandParams<'_>) -> Result<InstalledHooks, SlotError> {
    rendered_by("cursor-hooks-json", session).map(|rendered| {
        let (settings, text) = the_file(rendered);
        InstalledHooks {
            commands: settings["hooks"]
                .as_object()
                .map(|events| {
                    events
                        .iter()
                        .map(|(event, hooks)| {
                            (
                                event.clone(),
                                hooks[0]["command"].as_str().unwrap_or_default().to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            waiting: Vec::new(),
            names_a_tddy_binary: names_a_tddy_binary(&text),
        }
    })
}

/// The hooks Muse takes on its command line for `session`: `-c hooks=<json>`.
fn muse_hooks_for(session: &HookCommandParams<'_>) -> Result<InstalledHooks, SlotError> {
    rendered_by("muse-config-arg", session).map(|rendered| {
        let argv = match rendered {
            RenderedHooks::Argv(argv) => argv,
            RenderedHooks::File(path, _) => {
                panic!("the muse adapter rendered a file: {}", path.display())
            }
        };
        let value = argv
            .iter()
            .find_map(|entry| entry.strip_prefix("hooks="))
            .expect("the muse argv carries a hooks= entry");
        hooks_in_claude_shape(
            &serde_json::from_str(value).expect("the hooks= value is JSON"),
            &argv.join(" "),
        )
    })
}

/// What a session reports with: the commands of `events`, and whether anything names a tddy
/// binary.
fn reporting(
    hooks: Result<InstalledHooks, SlotError>,
    events: &[&str],
) -> Result<(BTreeMap<String, String>, bool), SlotError> {
    hooks.map(|hooks| (hooks.commands_of(events), hooks.names_a_tddy_binary))
}

#[test]
fn claude_code_on_a_target_gets_the_log_command_for_every_reporting_event() {
    // Given
    let session = a_session_on_a_target();

    // When
    let hooks = claude_hooks_for(&session);

    // Then
    assert_eq!(
        reporting(hooks, &REPORTING_EVENTS),
        Ok((the_log_command_for_each_of(&REPORTING_EVENTS), false))
    );
}

#[test]
fn cursor_on_a_target_gets_the_log_command_without_a_daemon_socket() {
    // Given a target session, which has no daemon socket to give
    let session = a_session_on_a_target();

    // When
    let hooks = cursor_hooks_for(&session);

    // Then
    assert_eq!(
        reporting(hooks, &CURSOR_EVENTS),
        Ok((the_log_command_for_each_of(&CURSOR_EVENTS), false))
    );
}

#[test]
fn muse_on_a_target_gets_the_log_command_on_its_command_line() {
    // Given a target session, which has no daemon socket to give
    let session = a_session_on_a_target();

    // When
    let hooks = muse_hooks_for(&session);

    // Then
    assert_eq!(
        reporting(hooks, &REPORTING_EVENTS),
        Ok((the_log_command_for_each_of(&REPORTING_EVENTS), false))
    );
}

#[test]
fn no_hook_that_waits_for_an_answer_is_installed_on_a_target() {
    // Given Claude Code, whose permission and question hooks wait for an answer on this host
    let session = a_session_on_a_target();

    // When
    let hooks = claude_hooks_for(&session);

    // Then neither is installed, and nothing installed waits
    let mut reporting_events = REPORTING_EVENTS.map(str::to_string).to_vec();
    reporting_events.sort_unstable();
    assert_eq!(
        hooks.map(|hooks| (hooks.events(), hooks.waiting)),
        Ok((reporting_events, Vec::new()))
    );
}

#[test]
fn a_local_session_still_gets_the_tools_command() {
    // Given a session on this host
    let session = a_local_session();

    // When
    let reports_through_the_tools: Vec<bool> = [
        claude_hooks_for(&session),
        cursor_hooks_for(&session),
        muse_hooks_for(&session),
    ]
    .into_iter()
    .map(|hooks| {
        hooks.is_ok_and(|hooks| {
            hooks.commands.len() >= CURSOR_EVENTS.len()
                && hooks
                    .commands
                    .values()
                    .all(|command| command.starts_with(&format!("'{TOOLS}' session-hook ")))
        })
    })
    .collect();

    // Then every hook of every adapter runs `tddy-tools session-hook`
    assert_eq!(reports_through_the_tools, vec![true, true, true]);
}

#[test]
fn a_local_claude_session_keeps_the_hooks_that_wait_for_an_answer() {
    // Given a session on this host
    let session = a_local_session();

    // When
    let waiting = claude_hooks_for(&session).map(|hooks| hooks.waiting);

    // Then
    assert_eq!(
        waiting,
        Ok(vec![
            "PermissionRequest".to_string(),
            "PreToolUse".to_string()
        ])
    );
}
