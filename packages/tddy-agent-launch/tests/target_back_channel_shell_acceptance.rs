//! Acceptance: the report command a child on a target is told to run works in the shell that runs
//! it there.
//!
//! The guidance a target child gets (`back_channel_guidance` over a `BackChannel::HookLog`) is a
//! shell line the agent's Bash tool runs in the **target user's login shell**. On the Coder
//! workspace `remote-dev` that shell is `/usr/bin/zsh` (2026-11-22 probe, session
//! `01a201cd-1cd1-7c53-8481-77991a117fac`): both of the child's reports reached `.tddy/hooks.log`
//! as `TddyMessage\t<token>\t` with an empty payload, and the follower dropped them.
//!
//! Findings: docs/dev/changesets/2026-11-22-coder-target-back-channel.md, problem 1.

use std::path::Path;
use std::process::Command;

use pretty_assertions::assert_eq;
use tddy_agent_launch::back_channel::{
    back_channel_for, back_channel_guidance, BackChannelInputs, ChildPlacement,
};
use tddy_agent_launch::remote_hook_follower::HookLine;
use tddy_coding_agent_specs::{embedded_spec, CodingAgentSpec};

const ORCHESTRATOR: &str = "01a16dc6-ab4c-7dc3-844e-baca4514f86e";
const HOOK_TOKEN: &str = "680361a1-c82f-43c3-8f24-5b0fb0525bb0";
const PLACEHOLDER: &str = "<your progress and status>";

fn agent(id: &str) -> CodingAgentSpec {
    embedded_spec(id).unwrap_or_else(|e| panic!("{id} is built in: {e}"))
}

/// The one shell line the guidance of a Claude child on a target tells it to run, with `report`
/// in place of the placeholder — exactly what the agent pastes into its Bash tool.
fn the_report_command_a_target_child_is_told_to_run(report: &str, worktree: &Path) -> String {
    let claude = agent("claude");
    let placement = ChildPlacement::of(Some(worktree), Some(HOOK_TOKEN))
        .expect("a target child with a hook token");
    let channel = back_channel_for(&BackChannelInputs {
        orchestrator_session_id: ORCHESTRATOR,
        orchestrator_agent: &claude,
        child_agent: &claude,
        placement,
    });
    let guidance = back_channel_guidance(ORCHESTRATOR, &channel);
    let line = guidance
        .lines()
        .find(|line| line.contains(PLACEHOLDER))
        .unwrap_or_else(|| panic!("the guidance names a command to run:\n{guidance}"));
    line.replace(PLACEHOLDER, report)
}

/// Run `command` with `shell -c` in `worktree`, as the agent's Bash tool runs it there, with no
/// terminal on stdin.
fn run_in(shell: &str, command: &str, worktree: &Path) {
    let status = Command::new(shell)
        .arg("-c")
        .arg(command)
        .current_dir(worktree)
        .stdin(std::process::Stdio::null())
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "{shell} must be installed to run this test (a Coder workspace's login shell): {e}"
            )
        });
    assert!(status.success(), "{shell} -c {command:?} failed: {status}");
}

/// The lines of the hook log in `worktree`.
fn hook_log_lines(worktree: &Path) -> Vec<String> {
    std::fs::read_to_string(worktree.join(tddy_core::REMOTE_HOOK_LOG))
        .expect("the report was appended to the hook log")
        .lines()
        .map(str::to_string)
        .collect()
}

/// A worktree on a target, as the daemon leaves it before the agent starts: an empty hook log.
fn a_target_worktree() -> tempfile::TempDir {
    let worktree = tempfile::tempdir().unwrap();
    let log = worktree.path().join(tddy_core::REMOTE_HOOK_LOG);
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, b"").unwrap();
    worktree
}

#[test]
fn a_report_run_from_a_zsh_login_shell_is_a_message_line_the_follower_accepts() {
    // Given a Claude child on a target whose login shell is zsh, and the command its guidance names
    let worktree = a_target_worktree();
    let command = the_report_command_a_target_child_is_told_to_run("Step 1 done.", worktree.path());

    // When the child runs it from its worktree
    run_in("zsh", &command, worktree.path());

    // Then the hook log holds one message line, carrying the report — not an empty payload the
    // follower drops as broken
    let lines = hook_log_lines(worktree.path());
    let texts: Vec<Option<String>> = lines
        .iter()
        .map(|line| {
            HookLine::parse(line, HOOK_TOKEN)
                .ok()
                .and_then(|hook| hook.payload["text"].as_str().map(str::to_string))
        })
        .collect();
    assert_eq!(
        texts,
        vec![Some("Step 1 done.".to_string())],
        "hook log lines: {lines:?}"
    );
}

#[test]
fn a_report_run_after_the_child_changed_directory_reaches_its_worktree_s_log() {
    // Given a Claude child on a target that has `cd`-ed into a subdirectory of its worktree, which
    // holds no `.tddy` of its own
    let worktree = a_target_worktree();
    let subdirectory = worktree.path().join("model-hound");
    std::fs::create_dir_all(&subdirectory).unwrap();
    let command = the_report_command_a_target_child_is_told_to_run("Step 2 done.", worktree.path());

    // When the child runs the command its guidance names from there
    run_in("sh", &command, &subdirectory);

    // Then the report is a message line in the worktree's hook log
    let texts: Vec<Option<String>> = hook_log_lines(worktree.path())
        .iter()
        .map(|line| {
            HookLine::parse(line, HOOK_TOKEN)
                .ok()
                .and_then(|hook| hook.payload["text"].as_str().map(str::to_string))
        })
        .collect();
    assert_eq!(
        (texts, subdirectory.join(".tddy").exists()),
        (vec![Some("Step 2 done.".to_string())], false)
    );
}
