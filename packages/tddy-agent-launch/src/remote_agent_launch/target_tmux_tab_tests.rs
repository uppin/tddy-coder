//! Unit tests: the tmux of a target login (`#target-tmux` 2/2) — the listing asks the target's login
//! shell for tmux, then for its sessions, and what each answer means; the tab command runs
//! `new-session -A` in the worktree through the login shell.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_cli_sessions::tmux::LIST_SESSIONS_FORMAT;
use tddy_remote_shell::{ExecOutput, InMemoryShell};
use tddy_terminal_rpc::TmuxSession;

use super::{list_target_tmux_sessions, target_tmux_tab_command};
use crate::remote_agent_launch::RemoteLaunchError;

const WORKTREE: &str = "/home/dev/repo/.worktrees/s1";

/// What the probe of tmux in [`WORKTREE`] runs on the target.
const PROBE_OF_TMUX: &str = r#"cd '/home/dev/repo/.worktrees/s1' && exec "${SHELL:-/bin/sh}" -lc 'command -v '"'"'tmux'"'"''"#;

/// What the listing of the target login's tmux sessions runs there.
fn listing_of_tmux() -> String {
    format!(
        r#"cd '/home/dev/repo/.worktrees/s1' && exec "${{SHELL:-/bin/sh}}" -lc 'exec tmux list-sessions -F '\''{LIST_SESSIONS_FORMAT}'\'''"#
    )
}

/// A reachable target named `buildbox` whose login shell finds tmux.
fn a_target_with_tmux() -> InMemoryShell {
    let target = InMemoryShell::new("buildbox");
    target.answers(PROBE_OF_TMUX, ExecOutput::succeeded("/usr/bin/tmux\n"));
    target
}

#[tokio::test]
async fn the_listing_reads_the_target_logins_tmux_sessions() {
    // Given a target whose tmux holds two sessions
    let target = a_target_with_tmux();
    target.answers(
        listing_of_tmux(),
        ExecOutput::succeeded("work\t2\t1\nscratch\t1\t0\n"),
    );

    // When
    let listed = list_target_tmux_sessions(&target, Path::new(WORKTREE)).await;

    // Then it asked for tmux, then for its sessions, and lists both
    assert_eq!(
        (listed, target.commands_run()),
        (
            Ok(vec![
                TmuxSession {
                    name: "work".to_string(),
                    windows: 2,
                    attached: 1,
                },
                TmuxSession {
                    name: "scratch".to_string(),
                    windows: 1,
                    attached: 0,
                },
            ]),
            vec![PROBE_OF_TMUX.to_string(), listing_of_tmux()]
        )
    );
}

#[tokio::test]
async fn a_target_running_no_tmux_server_lists_no_sessions() {
    // Given a target with tmux and no tmux server
    let target = a_target_with_tmux();
    target.answers(
        listing_of_tmux(),
        ExecOutput::exited(1, "no server running on /tmp/tmux-1000/default\n"),
    );

    // When
    let listed = list_target_tmux_sessions(&target, Path::new(WORKTREE)).await;

    // Then
    assert_eq!(listed, Ok(Vec::new()));
}

#[tokio::test]
async fn a_target_whose_login_shell_finds_no_tmux_lists_tmux_not_on_target() {
    // Given a target whose login shell finds no tmux
    let target = InMemoryShell::new("buildbox");
    target.answers(PROBE_OF_TMUX, ExecOutput::exited(1, ""));

    // When
    let listed = list_target_tmux_sessions(&target, Path::new(WORKTREE)).await;

    // Then it is refused naming tmux, and no listing was asked for
    assert_eq!(
        (listed, target.commands_run()),
        (
            Err(RemoteLaunchError::TmuxNotOnTarget),
            vec![PROBE_OF_TMUX.to_string()]
        )
    );
}

#[test]
fn a_tmux_tab_runs_new_session_attaching_or_making_it_in_the_worktree_through_the_login_shell() {
    // When
    let command = target_tmux_tab_command(Path::new(WORKTREE), "work", None);

    // Then
    assert_eq!(
        command,
        r#"cd '/home/dev/repo/.worktrees/s1' && exec "${SHELL:-/bin/sh}" -lc 'exec tmux new-session -A -s '\''work'\'''"#
    );
}

#[test]
fn a_tmux_tab_running_a_command_passes_it_as_the_new_sessions_command() {
    // When
    let command = target_tmux_tab_command(Path::new(WORKTREE), "tddy-s1-t1", Some("'claude'"));

    // Then
    assert_eq!(
        command,
        r#"cd '/home/dev/repo/.worktrees/s1' && exec "${SHELL:-/bin/sh}" -lc 'exec tmux new-session -A -s '\''tddy-s1-t1'\'' '\'''\''\'\'''\''claude'\''\'\'''\'''\'''"#
    );
}
