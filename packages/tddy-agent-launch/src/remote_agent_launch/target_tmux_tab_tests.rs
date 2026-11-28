//! Unit tests: the tmux of a target login (`#target-tmux` 2/2) — the listing asks the target's login
//! shell for tmux, then for its sessions, and what each answer means; the tab command runs
//! `new-session -A` in the worktree through the login shell.

// Test-only: included via `mod` under `#[cfg(test)]`, so the filesystem-port guard reads
// the whole file as test code and the real-disk fixtures below are the tests' fixtures, not
// production I/O.
#![cfg(test)]

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_cli_sessions::tmux::LIST_SESSIONS_FORMAT;
use tddy_remote_shell::{ExecOutput, InMemoryShell, RemoteShellError};
use tddy_terminal_rpc::TmuxSession;

use super::{
    enable_target_tmux_clipboard, list_target_tmux_sessions, target_agent_tmux_tab_command,
    target_tmux_tab_command,
};
use crate::remote_agent_launch::RemoteLaunchError;

const WORKTREE: &str = "/home/dev/repo/.worktrees/s1";

/// What the probe of tmux in [`WORKTREE`] runs on the target.
const PROBE_OF_TMUX: &str = r#"cd '/home/dev/repo/.worktrees/s1' && exec "${SHELL:-/bin/sh}" -lc 'command -v '"'"'tmux'"'"''"#;

/// What the listing of the target login's tmux sessions runs there.
fn listing_of_tmux() -> String {
    format!(
        r#"cd '/home/dev/repo/.worktrees/s1' && exec "${{SHELL:-/bin/sh}}" -lc 'exec tmux list-sessions -F '"'"'{LIST_SESSIONS_FORMAT}'"'"''"#
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
        r#"cd '/home/dev/repo/.worktrees/s1' && exec "${SHELL:-/bin/sh}" -lc 'exec tmux -T clipboard new-session -A -s '"'"'work'"'"''"#
    );
}

#[test]
fn a_tmux_tab_running_a_command_passes_it_as_the_new_sessions_command() {
    // When
    let command = target_tmux_tab_command(Path::new(WORKTREE), "tddy-s1-t1", Some("'claude'"));

    // Then
    assert_eq!(
        command,
        r#"cd '/home/dev/repo/.worktrees/s1' && exec "${SHELL:-/bin/sh}" -lc 'exec tmux -T clipboard new-session -A -s '"'"'tddy-s1-t1'"'"' '"'"''"'"'"'"'"'"'"'"'claude'"'"'"'"'"'"'"'"''"'"''"#
    );
}

#[test]
fn an_agent_tab_creates_or_attaches_with_clipboard_forwarding_enabled() {
    // When
    let command = target_agent_tmux_tab_command(Path::new(WORKTREE), "tddy-s1-t1", "'claude'");

    // Then it creates the session detached (a duplicate is fine — it attaches below), enables
    // forwarding on it, and attaches as a clipboard client
    assert_eq!(
        command,
        r#"cd '/home/dev/repo/.worktrees/s1' && exec "${SHELL:-/bin/sh}" -lc 'tmux new-session -d -s '"'"'tddy-s1-t1'"'"' '"'"''"'"'"'"'"'"'"'"'claude'"'"'"'"'"'"'"'"''"'"' 2>/dev/null; tmux set-option -t '"'"'=tddy-s1-t1:'"'"' allow-passthrough on 2>/dev/null; if [ "$(tmux show-options -s set-clipboard 2>/dev/null)" = "set-clipboard external" ]; then tmux set-option -s set-clipboard on 2>/dev/null; fi; exec tmux -T clipboard attach-session -t '"'"'=tddy-s1-t1'"'"''"#
    );
}

/// A stand-in `tmux` that records every call's arguments, then answers like a target's tmux:
/// `new-session -d` fails with `FAKE_NEW_SESSION_STATUS` (a duplicate complains on stderr),
/// `show-options` prints the `FAKE_SET_CLIPBOARD` value, `set-option` succeeds, and
/// `attach-session` names its session instead of attaching, so the command under test ends.
fn a_recording_tmux() -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().expect("a directory for the stand-in tmux");
    let log = dir.path().join("tmux.log");
    let tmux = dir.path().join("tmux");
    std::fs::write(
        &tmux,
        r#"#!/bin/sh
echo "$@" >> "$FAKE_TMUX_LOG"
while [ "${1#-}" != "$1" ]; do
  case "$1" in -T) shift 2;; *) shift;; esac
done
case "$1" in
  new-session) [ "${FAKE_NEW_SESSION_STATUS:-0}" = 0 ] || echo "duplicate session" >&2; exit "${FAKE_NEW_SESSION_STATUS:-0}";;
  show-options) echo "set-clipboard ${FAKE_SET_CLIPBOARD:-external}";;
  set-option) exit 0;;
  attach-session) echo "attached to $3";;
esac
"#,
    )
    .expect("the stand-in tmux is written");
    std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755))
        .expect("the stand-in tmux is made executable");
    // The target's login shell (`$SHELL -lc`), standing in without reading `/etc/profile`: on
    // macOS that runs `path_helper`, which puts the system's directories (and Homebrew's real
    // tmux) ahead of the stand-in's, so the recording tmux would never be called.
    let login_shell = dir.path().join("login-shell");
    std::fs::write(
        &login_shell,
        "#!/bin/sh\n[ \"$1\" = -lc ] || [ \"$1\" = -c ] || exit 64\nexec /bin/sh -c \"$2\"\n",
    )
    .expect("the stand-in login shell is written");
    std::fs::set_permissions(&login_shell, std::fs::Permissions::from_mode(0o755))
        .expect("the stand-in login shell is made executable");
    (dir, log)
}

/// What running `command` answers on a target with [`a_recording_tmux`] first on its `PATH`: its
/// output, and the tmux calls it made, in order.
fn run_with_a_recording_tmux(
    command: &str,
    extra_env: &[(&str, &str)],
) -> (std::process::Output, Vec<String>) {
    let (fake, log) = a_recording_tmux();
    let mut run = std::process::Command::new("/bin/sh");
    run.arg("-c")
        .arg(command)
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", fake.path().display()))
        .env("FAKE_TMUX_LOG", &log)
        .env("SHELL", fake.path().join("login-shell"));
    for (key, value) in extra_env {
        run.env(key, value);
    }
    let output = run.output().expect("a shell runs on this host");
    let calls = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    (output, calls)
}

/// A directory standing for a worktree on the target, by its path.
fn a_worktree() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temp dir");
    let worktree = dir.path().join("s1");
    std::fs::create_dir_all(&worktree).expect("the worktree exists");
    (dir, worktree)
}

#[test]
fn an_agent_tab_command_enables_forwarding_then_attaches() {
    // Given an agent tab's command on a target whose tmux holds the default clipboard
    let (_dir, worktree) = a_worktree();
    let command = target_agent_tmux_tab_command(&worktree, "tddy-s1-t1", "echo hi");

    // When the command runs there
    let (output, calls) = run_with_a_recording_tmux(&command, &[]);

    // Then it created the session, enabled forwarding on it, and attached to it
    assert_eq!(
        calls,
        vec![
            "new-session -d -s tddy-s1-t1 echo hi",
            "set-option -t =tddy-s1-t1: allow-passthrough on",
            "show-options -s set-clipboard",
            "set-option -s set-clipboard on",
            "-T clipboard attach-session -t =tddy-s1-t1",
        ]
    );
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("attached to =tddy-s1-t1"),
        "the client attached: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn an_agent_tab_command_leaves_an_explicitly_disabled_server_clipboard_off() {
    // Given an agent tab's command on a target whose clipboard a user hardened off
    let (_dir, worktree) = a_worktree();
    let command = target_agent_tmux_tab_command(&worktree, "tddy-s1-t1", "echo hi");

    // When the command runs there
    let (output, calls) = run_with_a_recording_tmux(&command, &[("FAKE_SET_CLIPBOARD", "off")]);

    // Then it passes through on the window but sets nothing server-wide, and attaches
    assert_eq!(
        calls,
        vec![
            "new-session -d -s tddy-s1-t1 echo hi",
            "set-option -t =tddy-s1-t1: allow-passthrough on",
            "show-options -s set-clipboard",
            "-T clipboard attach-session -t =tddy-s1-t1",
        ]
    );
    assert!(output.status.success());
}

#[test]
fn an_agent_tab_command_attaches_when_its_session_already_lives() {
    // Given an agent tab's command on a target whose session already lives
    let (_dir, worktree) = a_worktree();
    let command = target_agent_tmux_tab_command(&worktree, "tddy-s1-t1", "echo hi");

    // When the command runs there
    let (output, calls) = run_with_a_recording_tmux(&command, &[("FAKE_NEW_SESSION_STATUS", "1")]);

    // Then the duplicate-session complaint is silenced, forwarding is still enabled on the live
    // session, and the client attaches
    assert_eq!(
        calls,
        vec![
            "new-session -d -s tddy-s1-t1 echo hi",
            "set-option -t =tddy-s1-t1: allow-passthrough on",
            "show-options -s set-clipboard",
            "set-option -s set-clipboard on",
            "-T clipboard attach-session -t =tddy-s1-t1",
        ]
    );
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("attached to =tddy-s1-t1"),
        "the client attached: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// What enabling the clipboard on the target's tmux session `tddy-s1` runs there.
fn enablement_of_tddy_s1() -> String {
    r#"exec "${SHELL:-/bin/sh}" -lc 'tmux set-option -t '"'"'=tddy-s1:'"'"' allow-passthrough on 2>/dev/null; if [ "$(tmux show-options -s set-clipboard 2>/dev/null)" = "set-clipboard external" ]; then tmux set-option -s set-clipboard on 2>/dev/null; fi'"#
        .to_string()
}

#[tokio::test]
async fn enabling_the_clipboard_on_a_target_runs_through_its_login_shell() {
    // Given a target
    let target = InMemoryShell::new("buildbox");
    target.answers(enablement_of_tddy_s1(), ExecOutput::succeeded(""));

    // When
    let enabled = enable_target_tmux_clipboard(&target, "tddy-s1").await;

    // Then it ran the enablement there, and only there
    assert_eq!(
        (enabled, target.commands_run()),
        (Ok(()), vec![enablement_of_tddy_s1()])
    );
}

#[tokio::test]
async fn a_failed_enable_on_a_target_is_the_shells_error() {
    // Given a target whose enablement fails
    let target = InMemoryShell::new("buildbox");
    target.answers(
        enablement_of_tddy_s1(),
        ExecOutput::exited(1, "error connecting to /tmp/x (No such file)"),
    );

    // When
    let enabled = enable_target_tmux_clipboard(&target, "tddy-s1").await;

    // Then it is the shell's error
    assert_eq!(
        enabled,
        Err(RemoteLaunchError::Shell(RemoteShellError::Exited {
            status: 1,
            stderr: "error connecting to /tmp/x (No such file)".to_string(),
        }))
    );
}
