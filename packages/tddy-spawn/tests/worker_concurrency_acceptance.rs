//! Acceptance: the forked spawn worker serves requests concurrently, routes every answer to the
//! request it belongs to, reports events before a result, and fails every pending request when it
//! goes away.
//!
//! The worker loop runs on a thread of the test process over real pipes (`spawn_worker_over`), and
//! the client talks to it exactly as the daemon does (`client_over`). Clones run a fake `git` placed
//! on the child's `PATH` through the request's own environment, as the current OS user, so nothing
//! needs root and nothing touches the network.
//!
//! Feature docs: docs/ft/daemon/systemd-install.md (§ Spawn worker); packages/tddy-spawn/docs/spawn-worker.md
#![cfg(unix)]

use std::collections::BTreeMap;
use std::os::fd::FromRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tddy_spawn::spawn_worker::{
    client_over, spawn_worker_over, CloneRequest, SpawnClient, WorkerEvent,
};

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

fn the_current_os_user() -> String {
    let output = std::process::Command::new("id")
        .arg("-un")
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// A `git` that sleeps `$FAKE_GIT_SLEEP` seconds, fails naming its URL when the URL contains
/// `fail`, and otherwise creates its destination (its last argument).
fn a_fake_git_in(dir: &Path) -> PathBuf {
    let git = dir.join("git");
    std::fs::write(
        &git,
        "#!/bin/sh\n\
         sleep \"${FAKE_GIT_SLEEP:-0}\"\n\
         for last; do :; done\n\
         case \"$*\" in *fail*) echo \"cannot clone $*\" >&2; exit 1;; esac\n\
         mkdir -p \"$last\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir.to_path_buf()
}

struct AWorker {
    client: SpawnClient,
    fake_git_dir: tempfile::TempDir,
    repos: tempfile::TempDir,
    _worker: std::thread::JoinHandle<()>,
}

fn a_worker() -> AWorker {
    let (request_reader, request_writer) = a_pipe();
    let (response_reader, response_writer) = a_pipe();
    let worker = spawn_worker_over(request_reader, response_writer);
    let fake_git_dir = tempfile::tempdir().unwrap();
    a_fake_git_in(fake_git_dir.path());
    AWorker {
        client: client_over(request_writer, response_reader, 4242),
        fake_git_dir,
        repos: tempfile::tempdir().unwrap(),
        _worker: worker,
    }
}

impl AWorker {
    fn a_clone_of(&self, url: &str, name: &str, sleep_secs: u32) -> CloneRequest {
        let path = format!(
            "{}:{}",
            self.fake_git_dir.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        CloneRequest {
            os_user: the_current_os_user(),
            git_url: url.to_string(),
            destination: self.repos.path().join(name).display().to_string(),
            env: BTreeMap::from([
                ("PATH".to_string(), path),
                ("FAKE_GIT_SLEEP".to_string(), sleep_secs.to_string()),
            ]),
        }
    }
}

#[test]
fn a_fast_clone_completes_while_a_slow_clone_is_still_running() {
    // Given — a clone that takes ten seconds is in flight. The margins are wide on purpose: on a
    // loaded host each clone's `sh` and `id` spawns alone can take seconds, so the bound proves
    // "did not queue behind the slow clone", not "was fast".
    let worker = Arc::new(a_worker());
    let slow_request = worker.a_clone_of("https://example.invalid/slow.git", "slow", 10);
    let slow_client = worker.client.clone();
    let slow = std::thread::spawn(move || slow_client.clone_repo(slow_request));
    std::thread::sleep(Duration::from_millis(200));
    let fast_request = worker.a_clone_of("https://example.invalid/fast.git", "fast", 0);

    // When — a quick one is asked for
    let started = Instant::now();
    worker
        .client
        .clone_repo(fast_request)
        .expect("the quick clone succeeds");
    let fast_took = started.elapsed();

    // Then — it did not wait behind the slow one, which is still going
    assert!(
        fast_took < Duration::from_secs(5),
        "the quick clone took {fast_took:?}: it queued behind the ten-second one"
    );
    assert!(
        !slow.is_finished(),
        "the slow clone is still running after the quick one finished"
    );
    slow.join().unwrap().expect("the slow clone succeeds too");
}

#[test]
fn concurrent_requests_each_receive_their_own_result() {
    // Given — eight clones at once, every other one failing with its own URL in the error
    let worker = Arc::new(a_worker());
    let requests: Vec<(String, std::thread::JoinHandle<anyhow::Result<()>>)> = (0..8)
        .map(|n| {
            let url = if n % 2 == 0 {
                format!("https://example.invalid/ok-{n}.git")
            } else {
                format!("https://example.invalid/fail-{n}.git")
            };
            let request = worker.a_clone_of(&url, &format!("repo-{n}"), 1);
            let client = worker.client.clone();
            (url, std::thread::spawn(move || client.clone_repo(request)))
        })
        .collect();

    // When
    let results: Vec<(String, anyhow::Result<()>)> = requests
        .into_iter()
        .map(|(url, handle)| (url, handle.join().unwrap()))
        .collect();

    // Then — each failure names its own URL, and each success is one of the `ok` clones
    for (url, result) in results {
        let outcome = result.map_err(|e| format!("{e:#}"));
        if url.contains("fail") {
            let error = outcome.expect_err(&url);
            assert!(
                error.contains(&url),
                "{url} got someone else's error: {error}"
            );
        } else {
            outcome.unwrap_or_else(|e| panic!("{url} failed: {e}"));
        }
    }
}

#[test]
fn a_clone_reports_its_child_pid_before_its_result() {
    // Given
    let worker = a_worker();
    let mut events = Vec::new();

    // When
    worker
        .client
        .clone_repo_observed(
            worker.a_clone_of("https://example.invalid/observed.git", "observed", 0),
            |event| events.push(event),
        )
        .expect("the clone succeeds");

    // Then
    assert!(
        matches!(events.as_slice(), [WorkerEvent::Started { pid }] if *pid > 0),
        "exactly one started event carrying the child's pid: {events:?}"
    );
}

#[test]
fn every_pending_request_fails_naming_the_worker_when_it_exits() {
    // Given — a client whose worker reads its requests but never answers
    let (request_reader, request_writer) = a_pipe();
    let (response_reader, response_writer) = a_pipe();
    let client = client_over(request_writer, response_reader, 4242);
    let repos = tempfile::tempdir().unwrap();
    let pending: Vec<_> = (0..3)
        .map(|n| {
            let client = client.clone();
            let request = CloneRequest {
                os_user: the_current_os_user(),
                git_url: format!("https://example.invalid/{n}.git"),
                destination: repos.path().join(format!("{n}")).display().to_string(),
                env: BTreeMap::new(),
            };
            std::thread::spawn(move || client.clone_repo(request))
        })
        .collect();
    std::thread::sleep(Duration::from_millis(200));

    // When — the worker exits
    drop(response_writer);
    drop(request_reader);

    // Then — every request fails at once, naming the worker
    for handle in pending {
        let error = format!("{:#}", handle.join().unwrap().unwrap_err());
        assert!(
            error.contains("spawn worker pid=4242"),
            "a pending request names the worker that went away: {error}"
        );
    }
}

#[test]
fn the_worker_relays_clone_progress_before_the_clone_result() {
    // Given — a git that reports receiving objects before it finishes
    let worker = a_worker();
    std::fs::write(
        worker.fake_git_dir.path().join("git"),
        "#!/bin/sh\nfor last; do :; done\n\
         printf 'Receiving objects:  50%% (50/100), 1.00 MiB | 1.00 MiB/s\\r' >&2\n\
         printf 'Receiving objects: 100%% (100/100), 2.00 MiB | 1.00 MiB/s, done.\\n' >&2\n\
         mkdir -p \"$last\"\n",
    )
    .unwrap();
    let mut events = Vec::new();

    // When
    worker
        .client
        .clone_repo_observed(
            worker.a_clone_of("https://example.invalid/progress.git", "progress", 0),
            |event| events.push(event),
        )
        .expect("the clone succeeds");

    // Then
    assert!(
        events.iter().any(|event| matches!(
            event,
            WorkerEvent::CloneProgress(progress) if progress.stage == "Receiving objects" && progress.percent == 100
        )),
        "the clone's progress crossed the worker pipe: {events:?}"
    );
}

#[test]
fn every_worker_line_names_the_request_it_answers() {
    // Given
    let event = tddy_spawn::spawn_worker::WorkerLine::Event {
        id: 7,
        event: WorkerEvent::Started { pid: 4321 },
    };

    // When
    let line = serde_json::to_value(&event).unwrap();

    // Then — the id travels on the line itself, beside what it says
    assert_eq!(
        line,
        serde_json::json!({ "line": "event", "id": 7, "event": { "kind": "started", "pid": 4321 } })
    );
}
