//! A CLI harness's exit hook runs once its process has exited, with the terminal's whole output,
//! and a resume waiting on [`CliSessionManager::exit_hook_settled`] goes on only once the hook has
//! finished — so it reads what the hook recorded.
//!
//! The PTY is real: a stub harness binary that prints its last line and exits at once.
//!
//! Feature: docs/ft/daemon/warp-cli-session.md

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_cli_sessions::cli_session_manager::{CliSessionManager, ExitHook, HarnessLaunchExtras};
use tddy_core::cli_harness::{cli_harness, CliLaunch};
use tddy_testing_commons::wait::eventually_awaiting;
use tokio::sync::oneshot;

const SESSION: &str = "019fb0c1-exit-hook";

/// A safety net for the stub to exit, not a prediction of how long it takes.
const EXIT_OBSERVE: Duration = Duration::from_secs(10);

/// How long a wait that must not finish is given to finish anyway. The hook it waits on is held
/// until the test releases it, so no length makes this pass by luck.
const HELD: Duration = Duration::from_millis(200);

/// An executable that prints `last_line` and exits.
fn a_binary_printing(dir: &Path, last_line: &str) -> PathBuf {
    let path = dir.join("stub_harness.sh");
    std::fs::write(&path, format!("#!/bin/sh\nprintf '{last_line}'\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A hook that reports the output it was handed, then holds until `release` fires.
fn a_hook_held_until(release: oneshot::Receiver<()>) -> (ExitHook, oneshot::Receiver<String>) {
    let (seen_tx, seen_rx) = oneshot::channel();
    let hook: ExitHook = Box::new(move |capture| {
        Box::pin(async move {
            let output =
                String::from_utf8_lossy(capture.lock().unwrap().buffered_bytes()).into_owned();
            let _ = seen_tx.send(output);
            let _ = release.await;
        })
    });
    (hook, seen_rx)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resume_waits_for_the_exit_hook_of_the_run_before() {
    // Given: a harness that prints its last line and exits at once, launched with a held hook
    let dir = tempfile::tempdir().unwrap();
    let binary = a_binary_printing(dir.path(), "last-line-before-exit");
    let binary = binary.to_string_lossy();
    let manager = CliSessionManager::new();
    let (release_tx, release_rx) = oneshot::channel();
    let (hook, seen_rx) = a_hook_held_until(release_rx);
    manager
        .launch_cli_harness(
            cli_harness("warp-cli").expect("warp-cli"),
            SESSION,
            dir.path().to_path_buf(),
            &CliLaunch {
                binary: &binary,
                ..CliLaunch::default()
            },
            HarnessLaunchExtras {
                on_exit: Some(hook),
                ..HarnessLaunchExtras::default()
            },
            None,
        )
        .await
        .expect("harness spawned");

    // When: the process has exited and its hook is running
    let output = tokio::time::timeout(EXIT_OBSERVE, seen_rx)
        .await
        .expect("the exit hook never ran")
        .expect("the exit hook was dropped");

    // Then: the hook was handed everything the process printed
    assert!(
        output.contains("last-line-before-exit"),
        "the hook saw {output:?}"
    );
    // And a resume waits on it — the hook holds until it is released, so this cannot finish
    let mut settled = Box::pin(manager.exit_hook_settled(SESSION));
    assert!(
        tokio::time::timeout(HELD, &mut settled).await.is_err(),
        "a resume went on while the previous run's exit hook was still running"
    );

    // And when the hook finishes, the resume goes on and the terminal is deregistered
    release_tx.send(()).unwrap();
    tokio::time::timeout(EXIT_OBSERVE, settled)
        .await
        .expect("the resume never went on after the exit hook finished");
    eventually_awaiting("the terminal to be deregistered", EXIT_OBSERVE, || async {
        match manager.get(SESSION).await {
            None => Ok(()),
            Some(_) => Err("still registered".to_string()),
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resume_does_not_wait_for_a_run_that_is_still_going() {
    // Given: a harness still running, launched with an exit hook
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stub_harness.sh");
    std::fs::write(&path, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let binary = path.to_string_lossy();
    let manager = CliSessionManager::new();
    let (_release_tx, release_rx) = oneshot::channel();
    let (hook, _seen_rx) = a_hook_held_until(release_rx);
    manager
        .launch_cli_harness(
            cli_harness("warp-cli").expect("warp-cli"),
            SESSION,
            dir.path().to_path_buf(),
            &CliLaunch {
                binary: &binary,
                ..CliLaunch::default()
            },
            HarnessLaunchExtras {
                on_exit: Some(hook),
                ..HarnessLaunchExtras::default()
            },
            None,
        )
        .await
        .expect("harness spawned");

    // When / Then: nothing has been recorded yet, so there is nothing to wait for
    tokio::time::timeout(EXIT_OBSERVE, manager.exit_hook_settled(SESSION))
        .await
        .expect("a resume waited for a run that has not exited");
    manager.stop_session(SESSION).await;
}
