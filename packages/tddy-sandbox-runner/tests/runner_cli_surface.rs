//! The runner's command line knows no coding agent (`#agent-tools 10/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-tool-agnostic-jail.md`. Every jail is a tool plan: the
//! runner execs an `--exec-plan` written by the launcher, or serves `--workspace-tools`, and the
//! per-agent flags it used to build an agent's argv from are gone. Both tests drive the real binary,
//! and neither starts a jail: `--help` exits before any mode runs, and a refused flag is a parse
//! error.

use std::collections::BTreeSet;
use std::process::{Command, Output};

use pretty_assertions::assert_eq;

/// The flags the runner built an agent's argv from, before every jail ran a tool plan.
const AGENT_FLAGS: &[&str] = &[
    "--agent-kind",
    "--claude-binary",
    "--agent-binary",
    "--claude-arg",
    "--agent-arg",
    "--permission-mode",
    "--dangerously-skip-permissions",
    "--resume",
    "--append-system-prompt-file",
    "--mcp-log-level",
];

fn the_runner(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tddy-sandbox-runner"))
        .args(args)
        .output()
        .expect("the runner binary runs")
}

/// Every long flag `--help` lists: the first token of each option line.
fn long_flags_listed_by(help: &str) -> BTreeSet<String> {
    help.lines()
        .filter_map(|line| line.trim_start().split([' ', ',', '=']).next())
        .filter(|token| token.starts_with("--"))
        .map(str::to_string)
        .collect()
}

#[test]
fn the_runner_help_lists_no_agent_flag() {
    // Given
    let help = the_runner(&["--help"]);
    assert!(help.status.success(), "--help must succeed: {help:?}");
    let listed = long_flags_listed_by(&String::from_utf8_lossy(&help.stdout));

    // When
    let agent_flags_listed: Vec<&str> = AGENT_FLAGS
        .iter()
        .copied()
        .filter(|flag| listed.contains(*flag))
        .collect();

    // Then — the two modes a jail runs are still listed (positive control), no agent flag is
    assert!(
        listed.contains("--exec-plan") && listed.contains("--workspace-tools"),
        "the runner's modes must still be listed: {listed:#?}"
    );
    assert_eq!(agent_flags_listed, Vec::<&str>::new());
}

#[test]
fn passing_agent_kind_is_an_error() {
    // When
    let refused = the_runner(&["--agent-kind", "cursor"]);

    // Then
    assert_eq!(
        refused.status.code(),
        Some(2),
        "a clap usage error: {refused:?}"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.starts_with("error: unexpected argument '--agent-kind' found"),
        "the runner must refuse `--agent-kind` as an unknown flag, got:\n{stderr}"
    );
}
