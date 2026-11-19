//! Acceptance: following a target session's hook log leaves nothing running on the target once the
//! session's log is removed — even after the client that carried the follow was killed and the
//! command lived on, as a real ssh session's command does.
//!
//! The target is this machine (`LocalShell`), its client a shell that runs the follow as a child it
//! waits on, so killing the client orphans the command. `tail` is a stand-in that, like GNU `tail
//! -f` on a deleted log, never notices on its own that its reader is gone.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-remote-hooks.md.

use std::path::Path;
use std::time::{Duration, Instant};

use tddy_agent_launch::remote_hook_follower::follow_command;
use tddy_remote_shell::{LocalShell, RemoteShell};
use tddy_testing_commons::remote_target::write_executable;

/// Long enough for the follow's own watch on the log to notice it is gone; far less than forever.
const ENDS_WITHIN: Duration = Duration::from_secs(10);

/// Quote `arg` as one single-quoted POSIX shell word.
fn quoted(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

/// A `tail` that records its pid in `pid_file`, prints nothing, and runs until it is killed.
fn a_tail_that_never_ends_on_its_own(dir: &Path, pid_file: &Path) {
    write_executable(
        &dir.join("tail"),
        &format!(
            "#!/bin/sh\nprintf '%s' \"$$\" > {}\ntrap '' PIPE\nwhile :; do /bin/sleep 1; done\n",
            quoted(&pid_file.to_string_lossy())
        ),
    );
}

fn is_running(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .status()
        .is_ok_and(|status| status.success())
}

async fn recorded_pid(pid_file: &Path) -> String {
    let deadline = Instant::now() + ENDS_WITHIN;
    loop {
        let pid = std::fs::read_to_string(pid_file).unwrap_or_default();
        if !pid.trim().is_empty() {
            return pid.trim().to_string();
        }
        assert!(
            Instant::now() < deadline,
            "the follow never started its tail"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn the_follow_s_tail_ends_once_the_log_is_removed_though_its_client_was_killed() {
    // Given a session's hook log, followed through a client whose command outlives it
    let dir = tempfile::tempdir().expect("a directory standing for the target");
    let log = dir.path().join("hooks.log");
    std::fs::write(&log, b"").expect("the hook log");
    let pid_file = dir.path().join("tail.pid");
    a_tail_that_never_ends_on_its_own(dir.path(), &pid_file);
    let follow = follow_command(&log.to_string_lossy(), 0);
    let streamed = LocalShell::new()
        .exec_streamed(&format!(
            "PATH={}:\"$PATH\" /bin/sh -c {} & wait \"$!\"",
            quoted(&dir.path().to_string_lossy()),
            quoted(&follow)
        ))
        .await
        .expect("the follow starts");
    let tail = recorded_pid(&pid_file).await;

    // When the follower stops — its client is killed, the command lives on — and the session's
    // log is removed, as a session's end removes it
    drop(streamed);
    std::fs::remove_file(&log).expect("the log is removed");

    // Then the follow's tail ends of itself
    let deadline = Instant::now() + ENDS_WITHIN;
    while is_running(&tail) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let survived = is_running(&tail);
    if survived {
        let _ = std::process::Command::new("kill")
            .args(["-9", &tail])
            .status();
    }
    assert!(
        !survived,
        "the follow's tail (pid {tail}) outlived the session's log"
    );
}
