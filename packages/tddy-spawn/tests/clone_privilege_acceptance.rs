//! Acceptance: a clone as another OS user, asked of a process that is not root, fails at once with a
//! message naming both users — never a child stuck before `exec` (the 2026-10-24 incident).
//!
//! Every developer machine and CI runner runs these tests unprivileged, and `root` exists on both, so
//! asking for a clone as `root` is the refused case.
//!
//! The clone runs on its own thread and the test waits a bounded time for it: before the fix the
//! child deadlocks in `pre_exec` and the clone never returns, which must fail this test rather than
//! hang the suite.
//!
//! Feature docs: docs/ft/daemon/1-WIP/PRD-2026-10-24-start-progress-spawn-diag.md
#![cfg(unix)]

use std::time::{Duration, Instant};

fn the_current_os_user() -> String {
    let output = std::process::Command::new("id")
        .arg("-un")
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn a_clone_as_another_user_from_a_non_root_process_fails_fast_naming_both_users() {
    // Given
    let repos = tempfile::tempdir().unwrap();
    let destination = repos.path().join("tddy-coder");
    let started = Instant::now();
    let (finished, outcome) = std::sync::mpsc::channel();
    let target = destination.clone();

    // When
    std::thread::spawn(move || {
        let _ = finished.send(tddy_spawn::spawner::clone_as_user_with_env(
            "root",
            "https://example.invalid/never-fetched.git",
            &target,
            &[],
        ));
    });
    let error = outcome
        .recv_timeout(Duration::from_secs(5))
        .expect("the clone returns instead of hanging before exec")
        .expect_err("an unprivileged process cannot clone as root");

    // Then
    let message = format!("{error:#}");
    assert!(
        message.contains("'root'") && message.contains(&format!("'{}'", the_current_os_user())),
        "the error names the requested and the current user: {message}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "refused before any fork, not after a wait: took {:?}",
        started.elapsed()
    );
    assert!(
        !destination.exists(),
        "nothing was created at the destination"
    );
}
