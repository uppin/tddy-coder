//! A session's agent terminal is recorded to the path registered for it: periodically while it
//! runs, and at exit before the terminal leaves the index.
//!
//! The PTY is real: a stub agent prints what it is told to and exits on a newline from stdin.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-02-agent-terminal-history-persists.md (`#session-tabs 2/7`)

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tddy_cli_sessions::cli_session_manager::{CliSessionManager, PtyHandle};
use tddy_task::CaptureRecording;
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

const SESSION: &str = "019fb0c1-capture-recorder";

/// A safety net for the stub to print and exit, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(10);

/// An agent that prints `first`, then on a newline prints `last` and exits.
fn an_agent_printing(dir: &Path, first: &str, last: &str) -> PathBuf {
    let path = dir.join("stub_agent.sh");
    std::fs::write(
        &path,
        format!("#!/bin/sh\nprintf '{first}'\nread _line\nprintf '{last}'\nexit 0\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

async fn start_main(manager: &CliSessionManager, dir: &Path, agent: &Path) -> Arc<PtyHandle> {
    manager
        .start(
            SESSION,
            dir.to_path_buf(),
            "claude-opus-4-8",
            agent.to_str().unwrap(),
            None,
            None,
        )
        .await
        .expect("main terminal starts")
}

fn let_it_exit(main: &PtyHandle) {
    main.stdin_tx.send(Bytes::from_static(b"\n")).unwrap();
}

/// The recording's bytes as text, once the file exists and holds `expected`.
async fn recording_containing(path: &Path, expected: &str) -> String {
    eventually("the recording to hold the expected output", OBSERVE, || {
        let text = CaptureRecording::read(path)
            .map(|recording| String::from_utf8_lossy(&recording.data).into_owned())
            .map_err(|e| format!("{} is not a readable recording yet: {e}", path.display()))?;
        if text.contains(expected) {
            Ok(text)
        } else {
            Err(format!(
                "the recording holds {text:?}, not yet {expected:?}"
            ))
        }
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_registered_main_is_recorded_with_its_last_output_before_it_leaves_the_index() {
    // Given a running agent whose terminal is recorded
    let dir = tempfile::tempdir().unwrap();
    let recording = dir.path().join("terminals").join("main.capture");
    std::fs::create_dir_all(recording.parent().unwrap()).unwrap();
    let manager = CliSessionManager::new();
    let main = start_main(
        &manager,
        dir.path(),
        &an_agent_printing(dir.path(), "working", "last-words"),
    )
    .await;
    manager
        .record_main_capture(SESSION, recording.clone())
        .await;

    // When the agent exits
    let_it_exit(&main);
    eventually_awaiting("main to leave the index", OBSERVE, || async {
        if manager.get(SESSION).await.is_none() {
            Ok(())
        } else {
            Err("main is still registered".to_string())
        }
    })
    .await;

    // Then the recording already holds its last output
    let text = String::from_utf8_lossy(
        &CaptureRecording::read(&recording)
            .expect("the recording was written before main left the index")
            .data,
    )
    .into_owned();
    assert!(text.contains("last-words"), "{text:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_main_is_recorded_on_the_interval() {
    // Given an agent that keeps running, recorded every 50ms
    let dir = tempfile::tempdir().unwrap();
    let recording = dir.path().join("main.capture");
    let manager = CliSessionManager::new().with_recording_interval(Duration::from_millis(50));
    let main = start_main(
        &manager,
        dir.path(),
        &an_agent_printing(dir.path(), "still-working", "done"),
    )
    .await;

    // When its terminal is registered for recording
    manager
        .record_main_capture(SESSION, recording.clone())
        .await;

    // Then its output is on disk while it still runs
    let text = recording_containing(&recording, "still-working").await;
    assert!(!text.contains("done"), "{text:?}");
    assert!(
        manager.get(SESSION).await.is_some(),
        "main must still be running"
    );
    let_it_exit(&main);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unregistered_session_is_never_recorded() {
    // Given an agent nobody registered a recording for
    let dir = tempfile::tempdir().unwrap();
    let manager = CliSessionManager::new().with_recording_interval(Duration::from_millis(50));
    let main = start_main(
        &manager,
        dir.path(),
        &an_agent_printing(dir.path(), "working", "bye"),
    )
    .await;

    // When it exits
    let_it_exit(&main);
    eventually_awaiting("main to leave the index", OBSERVE, || async {
        if manager.get(SESSION).await.is_none() {
            Ok(())
        } else {
            Err("main is still registered".to_string())
        }
    })
    .await;

    // Then nothing named like a recording was written next to it
    let written: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "capture"))
        .collect();
    assert!(written.is_empty(), "{written:?}");
}
