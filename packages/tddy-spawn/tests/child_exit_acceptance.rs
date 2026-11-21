//! Acceptance: whoever holds a spawned tool reports its end, with its exit code — the forked spawn
//! worker through `WorkerEvent::Exited`, the direct spawner through its wait thread
//! (`#session-liveness` 3/4, PRD AC5) — and a signal to it reaches the child while it runs and is
//! never delivered once it has ended, when its pid may name another process (`#tab-hardening` 2/4).
//!
//! The tool is a stand-in `tddy-coder` that outlives the startup watch, waits for the test to
//! release it, and exits with a code of its own. The worker loop runs on a thread of the test
//! process over real pipes, as in `worker_concurrency_acceptance.rs`. Everything runs as the current
//! OS user.
//!
//! PRD: docs/ft/daemon/session-liveness.md
#![cfg(unix)]

use std::os::fd::FromRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_spawn::child_exit::{spawn_tool_as_user, ChildExit, SpawnedTool};
use tddy_spawn::child_signal::{Signalled, ToolSignal};
use tddy_spawn::spawn_worker::{build_spawn_request, client_over, spawn_worker_over, SpawnClient};
use tddy_spawn::spawner::{self, LiveKitCreds, SpawnOptions, StartupWatch};

/// The code the stand-in exits with once released.
const EXIT_CODE: i32 = 3;

/// A safety net for a released child's end to be reported, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(10);

fn a_pipe() -> (std::fs::File, std::fs::File) {
    let mut fds = [0; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "pipe(2)");
    unsafe {
        (
            std::fs::File::from_raw_fd(fds[0]),
            std::fs::File::from_raw_fd(fds[1]),
        )
    }
}

fn current_username() -> String {
    std::env::var("USER").expect("USER must be set to spawn as the current user")
}

fn a_livekit() -> LiveKitCreds {
    LiveKitCreds {
        url: "ws://127.0.0.1:7880".to_string(),
        api_key: "test-key".to_string(),
        api_secret: "test-secret".to_string(),
        common_room: None,
        daemon_instance_id: None,
    }
}

fn a_startup_watch() -> StartupWatch {
    StartupWatch::from_millis(300, 10)
}

/// A stand-in `tddy-coder` and the places it runs in.
struct ATool {
    path: PathBuf,
    release: PathBuf,
    data_dir: tempfile::TempDir,
    repo: tempfile::TempDir,
    _dir: tempfile::TempDir,
}

/// A tool that waits until `release` exists, then exits with [`EXIT_CODE`].
fn a_tool_that_exits_with_a_code_once_released() -> ATool {
    let dir = tempfile::tempdir().unwrap();
    let release = dir.path().join("release");
    let path = dir.path().join("fake-tddy-coder.sh");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nuntil [ -e \"{}\" ]; do sleep 0.05; done\nexit {EXIT_CODE}\n",
            release.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    ATool {
        path,
        release,
        data_dir: tempfile::tempdir().unwrap(),
        repo: tempfile::tempdir().unwrap(),
        _dir: dir,
    }
}

/// A tool a test did not release — one whose signal never arrived — is released as the test ends,
/// so it exits instead of waiting on a file in a directory that is gone.
impl Drop for ATool {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.release, "");
    }
}

impl ATool {
    fn releases(&self) {
        std::fs::write(&self.release, "").unwrap();
    }

    fn tool_path(&self) -> &str {
        self.path.to_str().unwrap()
    }

    fn data_dir(&self) -> &Path {
        self.data_dir.path()
    }

    fn repo(&self) -> &Path {
        self.repo.path()
    }
}

fn a_worker() -> (SpawnClient, std::thread::JoinHandle<()>) {
    let (request_reader, request_writer) = a_pipe();
    let (response_reader, response_writer) = a_pipe();
    let worker = spawn_worker_over(request_reader, response_writer);
    (client_over(request_writer, response_reader, 4242), worker)
}

/// The end the spawn reports once the tool is released.
async fn the_end_reported_after_releasing(tool: &ATool, spawned: SpawnedTool) -> ChildExit {
    tool.releases();
    tokio::time::timeout(OBSERVE, spawned.exit.exited())
        .await
        .expect("the child's end is reported")
        .expect("the holder reports the end before going away")
}

/// A tool spawned through `client`, waiting to be released.
async fn a_tool_spawned_through_the_worker(tool: &ATool, client: SpawnClient) -> SpawnedTool {
    let request = build_spawn_request(
        &current_username(),
        tool.tool_path(),
        tool.data_dir(),
        tool.repo(),
        &a_livekit(),
        SpawnOptions {
            new_session_id: Some("worker-session"),
            ..Default::default()
        },
        None,
        None,
        a_startup_watch(),
    );
    tokio::task::spawn_blocking(move || client.spawn_tool(request))
        .await
        .unwrap()
        .expect("the worker spawns the tool")
}

/// A tool spawned directly, as the current user, waiting to be released.
fn a_tool_spawned_directly(tool: &ATool) -> SpawnedTool {
    spawn_tool_as_user(
        &current_username(),
        tool.tool_path(),
        tool.data_dir(),
        tool.repo(),
        &a_livekit(),
        SpawnOptions {
            new_session_id: Some("direct-session"),
            ..Default::default()
        },
        "info",
        spawner::CHILD_LOG_FORMAT_FALLBACK,
        None,
        a_startup_watch(),
    )
    .expect("the tool is spawned")
}

/// How the spawn says its child is signalled.
fn its_signal(spawned: &SpawnedTool) -> ToolSignal {
    spawned
        .signal
        .clone()
        .expect("a child the daemon or its worker is the parent of can be signalled")
}

/// Send `signal` through `to`, off the executor: a worker's answer is waited for.
async fn sends(to: ToolSignal, signal: i32) -> Signalled {
    tokio::task::spawn_blocking(move || to.send(signal))
        .await
        .unwrap()
        .expect("the signal is answered")
}

/// The end the spawn reports without being released.
async fn the_end_reported(spawned: SpawnedTool) -> ChildExit {
    tokio::time::timeout(OBSERVE, spawned.exit.exited())
        .await
        .expect("the child's end is reported")
        .expect("the holder reports the end before going away")
}

#[tokio::test]
async fn a_signal_to_a_running_direct_child_reaches_it() {
    // Given a directly spawned tool that has not been released
    let tool = a_tool_that_exits_with_a_code_once_released();
    let spawned = a_tool_spawned_directly(&tool);
    let signal = its_signal(&spawned);

    // When it is sent SIGTERM
    let outcome = sends(signal, libc::SIGTERM).await;

    // Then the signal was delivered, and it is what ended the child
    assert_eq!(
        (outcome, the_end_reported(spawned).await),
        (Signalled::Delivered, ChildExit::by_signal(libc::SIGTERM))
    );
}

#[tokio::test]
async fn a_signal_to_an_ended_direct_child_is_not_delivered() {
    // Given a directly spawned tool whose end has been reported
    let tool = a_tool_that_exits_with_a_code_once_released();
    let spawned = a_tool_spawned_directly(&tool);
    let signal = its_signal(&spawned);
    the_end_reported_after_releasing(&tool, spawned).await;

    // When it is sent SIGTERM
    let outcome = sends(signal, libc::SIGTERM).await;

    // Then nothing is sent to a pid that may name another process by now
    assert_eq!(outcome, Signalled::ChildEnded);
}

#[tokio::test]
async fn a_signal_to_a_running_worker_child_reaches_it() {
    // Given a tool spawned through the worker that has not been released
    let tool = a_tool_that_exits_with_a_code_once_released();
    let (client, _worker) = a_worker();
    let spawned = a_tool_spawned_through_the_worker(&tool, client).await;
    let signal = its_signal(&spawned);

    // When it is sent SIGTERM
    let outcome = sends(signal, libc::SIGTERM).await;

    // Then the worker delivered it, and it is what ended the child
    assert_eq!(
        (outcome, the_end_reported(spawned).await),
        (Signalled::Delivered, ChildExit::by_signal(libc::SIGTERM))
    );
}

#[tokio::test]
async fn a_signal_to_an_ended_worker_child_is_not_delivered() {
    // Given a tool spawned through the worker whose end the worker has reported
    let tool = a_tool_that_exits_with_a_code_once_released();
    let (client, _worker) = a_worker();
    let spawned = a_tool_spawned_through_the_worker(&tool, client).await;
    let signal = its_signal(&spawned);
    the_end_reported_after_releasing(&tool, spawned).await;

    // When it is sent SIGTERM
    let outcome = sends(signal, libc::SIGTERM).await;

    // Then the worker — the child's parent, which reaped it — sends nothing
    assert_eq!(outcome, Signalled::ChildEnded);
}

#[tokio::test]
async fn the_forked_worker_reports_its_childs_exit_code() {
    // Given a tool spawned through the worker
    let tool = a_tool_that_exits_with_a_code_once_released();
    let (client, _worker) = a_worker();
    let request = build_spawn_request(
        &current_username(),
        tool.tool_path(),
        tool.data_dir(),
        tool.repo(),
        &a_livekit(),
        SpawnOptions {
            new_session_id: Some("worker-session"),
            ..Default::default()
        },
        None,
        None,
        a_startup_watch(),
    );
    let spawned = tokio::task::spawn_blocking(move || client.spawn_tool(request))
        .await
        .unwrap()
        .expect("the worker spawns the tool");

    // When the tool exits
    let end = the_end_reported_after_releasing(&tool, spawned).await;

    // Then the worker reported its exit code
    assert_eq!(end, ChildExit::with_code(EXIT_CODE));
}

#[tokio::test]
async fn the_direct_spawner_reports_its_childs_exit_code() {
    // Given a tool spawned directly, as the current user
    let tool = a_tool_that_exits_with_a_code_once_released();
    let spawned = spawn_tool_as_user(
        &current_username(),
        tool.tool_path(),
        tool.data_dir(),
        tool.repo(),
        &a_livekit(),
        SpawnOptions {
            new_session_id: Some("direct-session"),
            ..Default::default()
        },
        "info",
        spawner::CHILD_LOG_FORMAT_FALLBACK,
        None,
        a_startup_watch(),
    )
    .expect("the tool is spawned");

    // When the tool exits
    let end = the_end_reported_after_releasing(&tool, spawned).await;

    // Then its wait thread reported the exit code
    assert_eq!(end, ChildExit::with_code(EXIT_CODE));
}
