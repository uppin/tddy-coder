//! Unit tests: the hooks a start or resume on a target installs keep reaching the session's hook
//! log wherever the agent's current directory is.
//!
//! Claude Code runs a hook in the agent's current directory, which follows the agent's `cd`. Seen
//! live on 2026-11-24 (Coder workspace `remote-dev`, session
//! `01a20c4d-8862-7201-81da-5f6d0ab37656`): once the agent had `cd`-ed into `model-hound/`, every
//! hook failed with `cannot create .tddy/hooks.log: Directory nonexistent`.

use super::*;
use pretty_assertions::assert_eq;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const HOOK_TOKEN: &str = "tok-7f3a9c";

/// A session's worktree on a target, as the launch leaves it before the agent starts: its hook
/// log's directory made.
struct ATargetWorktree {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

fn a_target_worktree() -> ATargetWorktree {
    a_target_worktree_named("worktree")
}

fn a_target_worktree_named(name: &str) -> ATargetWorktree {
    let dir = tempfile::tempdir().expect("a directory standing for the target");
    let root = dir.path().join(name);
    std::fs::create_dir_all(root.join(".tddy")).expect("the worktree's .tddy directory");
    ATargetWorktree { root, _dir: dir }
}

impl ATargetWorktree {
    /// A subdirectory of the worktree the agent has `cd`-ed into, holding no `.tddy` of its own.
    fn subdirectory(&self, name: &str) -> PathBuf {
        let subdirectory = self.root.join(name);
        std::fs::create_dir_all(&subdirectory).expect("the agent's subdirectory");
        subdirectory
    }

    /// The events the session's hook log holds, in order.
    fn logged_events(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join(tddy_core::REMOTE_HOOK_LOG))
            .unwrap_or_default()
            .lines()
            .map(|line| line.split('\t').next().unwrap_or_default().to_string())
            .collect()
    }
}

/// A claude start's hooks, pointed at the hook log of `worktree` on its target.
fn hooks_of_a_claude_session_on_a_target(worktree: &ATargetWorktree) -> ClaudeCliHooks {
    let claude = tddy_coding_agent_specs::embedded_spec("claude").expect("claude.yaml is built in");
    let mut hooks = ClaudeCliHooks {
        tddy_tools_path: String::new(),
        daemon_url: String::new(),
        daemon_socket: None,
        hook_token: HOOK_TOKEN.to_string(),
        answer_wait_secs: claude.answer_wait_secs(),
        remote_log: None,
        route_env: Vec::new(),
    };
    hooks.point_at_the_target_log(&worktree.root);
    hooks
}

/// The command `hooks` install for the `index`-th matcher group of `event` in Claude's settings.
fn installed_command(hooks: &ClaudeCliHooks, event: &str, index: usize) -> String {
    let settings = tddy_core::build_claude_hooks_settings(&hooks.params("s1", "dev"));
    settings["hooks"][event][index]["hooks"][0]["command"]
        .as_str()
        .unwrap_or_else(|| panic!("no {event} hook is installed: {settings}"))
        .to_string()
}

/// Run `command` as Claude Code runs a hook: under `sh -c` in `dir`, `payload` on its stdin.
/// Whether it exited zero.
fn fire_in(dir: &Path, command: &str, payload: &str) -> bool {
    fire_with("/bin/sh", dir, command, payload)
}

/// [`fire_in`], under `shell -c`.
fn fire_with(shell: &str, dir: &Path, command: &str, payload: &str) -> bool {
    let mut hook = Command::new(shell)
        .arg("-c")
        .arg(command)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("{shell} must be installed to run this test: {e}"));
    hook.stdin
        .take()
        .expect("the hook's stdin is piped")
        .write_all(payload.as_bytes())
        .expect("the hook reads its payload");
    hook.wait().expect("the hook exits").success()
}

#[test]
fn a_target_session_s_hooks_reach_its_log_after_the_agent_changed_directory() {
    // Given a claude session on a target whose agent has `cd`-ed into a subdirectory
    let worktree = a_target_worktree();
    let hooks = hooks_of_a_claude_session_on_a_target(&worktree);
    let subdirectory = worktree.subdirectory("model-hound");

    // When its next tool call and the turn's end fire their hooks there
    let reported_the_tool = fire_in(
        &subdirectory,
        &installed_command(&hooks, "PreToolUse", 0),
        r#"{"hook_event_name":"PreToolUse","tool_name":"Bash"}"#,
    );
    let reported_the_end = fire_in(
        &subdirectory,
        &installed_command(&hooks, "Stop", 0),
        r#"{"hook_event_name":"Stop"}"#,
    );

    // Then both appended to the worktree's log, and nothing was written beside the agent
    assert_eq!(
        (
            reported_the_tool,
            reported_the_end,
            worktree.logged_events(),
            subdirectory.join(".tddy").exists(),
        ),
        (
            true,
            true,
            vec!["PreToolUse".to_string(), "Stop".to_string()],
            false
        )
    );
}

#[test]
fn a_target_session_s_waiting_hook_waits_for_its_answer_in_the_worktree() {
    // Given a claude session on a target
    let worktree = a_target_worktree();
    let hooks = hooks_of_a_claude_session_on_a_target(&worktree);

    // When its permission hook is installed
    let command = installed_command(&hooks, "PermissionRequest", 0);

    // Then it waits for the answer in the worktree's answers directory, wherever the agent is
    let answers = worktree.root.join(tddy_core::REMOTE_ANSWERS_DIR);
    assert!(
        command.contains(&format!("'{}'/", answers.display())),
        "the waiting hook reads its answer from {}: {command}",
        answers.display()
    );
}

#[test]
fn a_worktree_path_with_spaces_and_quotes_still_takes_every_hook() {
    // Given a claude session on a target whose worktree path holds a space and single quotes, and
    // whose agent has `cd`-ed into a subdirectory whose name holds a space
    let worktree = a_target_worktree_named("o'brien's repo");
    let hooks = hooks_of_a_claude_session_on_a_target(&worktree);
    let subdirectory = worktree.subdirectory("model hound");

    // When the turn's end fires its hook there
    let reported = fire_in(
        &subdirectory,
        &installed_command(&hooks, "Stop", 0),
        r#"{"hook_event_name":"Stop"}"#,
    );

    // Then it appended to the worktree's log
    assert_eq!(
        (reported, worktree.logged_events()),
        (true, vec!["Stop".to_string()])
    );
}

#[test]
fn every_posix_shell_a_target_may_run_hooks_under_appends_from_a_subdirectory() {
    // Given a claude session on a target whose agent has `cd`-ed into a subdirectory
    let worktree = a_target_worktree();
    let hooks = hooks_of_a_claude_session_on_a_target(&worktree);
    let subdirectory = worktree.subdirectory("model-hound");
    let command = installed_command(&hooks, "Stop", 0);

    // When the hook runs under dash (a Debian or Ubuntu target's `/bin/sh`, Coder's `remote-dev`
    // among them), bash, and this host's `/bin/sh`
    let shells = ["dash", "bash", "/bin/sh"];
    let reported: Vec<(&str, bool)> = shells
        .iter()
        .map(|shell| {
            (
                *shell,
                fire_with(
                    shell,
                    &subdirectory,
                    &command,
                    r#"{"hook_event_name":"Stop"}"#,
                ),
            )
        })
        .collect();

    // Then each appended its line to the worktree's log
    assert_eq!(
        (reported, worktree.logged_events()),
        (
            shells.iter().map(|shell| (*shell, true)).collect(),
            vec!["Stop".to_string(); 3]
        )
    );
}
