//! A clone the supervisor runs for the daemon reports git's progress, ends with how git exited and
//! git's own explanation of a failure, and runs under the same user and environment policy as a
//! session spawn.
//!
//! Real `git` clones a generated repository over `file://` (so git packs and reports transfer
//! progress), as the account running the tests. Every test needs the supervisor to fork for real, so
//! the whole file is Linux-only — see the note in `support`'s header.
//!
//! Feature docs: docs/ft/daemon/1-WIP/PRD-2026-10-24-start-progress-supervised-clone-progress.md
#![cfg(target_os = "linux")]

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use support::{a_service, a_supervisor, current_username, RunningSupervisor};
use tddy_supervisor::proto::supervisor::clone_event::Event;
use tddy_supervisor::proto::supervisor::{CloneExited, CloneRepositoryRequest};

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn a_repository_worth_cloning(dir: &Path) -> String {
    git(dir, &["init", "-q", "-b", "main"]);
    for n in 0..300 {
        std::fs::write(
            dir.join(format!("file-{n}.txt")),
            format!("{n}\n").repeat(200),
        )
        .unwrap();
    }
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "seed"]);
    format!("file://{}", dir.display())
}

/// The `git` the supervisor will resolve: the first on `PATH`, which it inherits from this process.
fn the_git_on_path() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").expect("a PATH"))
        .map(|dir| dir.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("git on PATH")
}

/// The clone runs git, and git is a tool like any other: the policy has to list it.
async fn a_supervisor_allowing_the_current_user() -> RunningSupervisor {
    a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(&the_git_on_path())
        .start()
        .await
}

fn a_clone_of(url: &str, destination: &Path) -> CloneRepositoryRequest {
    CloneRepositoryRequest {
        os_user: current_username(),
        git_url: url.to_string(),
        destination: destination.display().to_string(),
        env: Default::default(),
    }
}

/// The clone's pid, every progress line it reported, and how it ended.
async fn clone_through(
    supervisor: &RunningSupervisor,
    request: CloneRepositoryRequest,
) -> (u32, Vec<String>, CloneExited) {
    let mut events = supervisor
        .client()
        .await
        .clone_repository(request)
        .await
        .expect("the clone starts");
    let mut pid = 0;
    let mut lines = Vec::new();
    while let Some(event) = tokio::time::timeout(Duration::from_secs(60), events.recv())
        .await
        .expect("an event within a minute")
    {
        match event.expect("a clone event").event.expect("a variant") {
            Event::Started(started) => pid = started.pid,
            Event::Progress(progress) => lines.push(progress.line),
            Event::Exited(exited) => return (pid, lines, exited),
        }
    }
    panic!("the clone's stream ended without saying how git exited; lines: {lines:?}");
}

#[tokio::test]
async fn a_supervised_clone_reports_progress_then_exits_zero() {
    // Given
    let source = tempfile::tempdir().unwrap();
    let url = a_repository_worth_cloning(source.path());
    let repos = tempfile::tempdir().unwrap();
    let destination = repos.path().join("project");
    let supervisor = a_supervisor_allowing_the_current_user().await;

    // When
    let (pid, lines, exited) = clone_through(&supervisor, a_clone_of(&url, &destination)).await;

    // Then
    assert_ne!(pid, 0, "the first event named the clone's pid");
    assert_eq!(exited.code, Some(0), "git exited cleanly: {exited:?}");
    assert!(
        lines.iter().any(|line| line.contains("Receiving objects")),
        "git's progress was reported: {lines:?}"
    );
    assert!(
        destination.join(".git").is_dir(),
        "the clone landed at its destination"
    );
    assert_eq!(
        std::fs::read_dir(repos.path()).unwrap().count(),
        1,
        "no temporary directory left beside it"
    );
}

#[tokio::test]
async fn a_failed_supervised_clone_ends_with_gits_stderr_tail() {
    // Given
    let repos = tempfile::tempdir().unwrap();
    let destination = repos.path().join("project");
    let supervisor = a_supervisor_allowing_the_current_user().await;

    // When
    let (_, _, exited) = clone_through(
        &supervisor,
        a_clone_of("file:///nonexistent/tddy-no-such-repository", &destination),
    )
    .await;

    // Then
    assert_ne!(exited.code, Some(0), "git failed: {exited:?}");
    assert!(
        exited.stderr_tail.contains("tddy-no-such-repository"),
        "the failure carries git's own words: {exited:?}"
    );
    assert_eq!(
        std::fs::read_dir(repos.path()).unwrap().count(),
        0,
        "nothing left behind"
    );
}

#[tokio::test]
async fn a_supervised_clone_with_an_unlisted_env_key_is_refused() {
    // Given
    let repos = tempfile::tempdir().unwrap();
    let supervisor = a_supervisor_allowing_the_current_user().await;
    let request = CloneRepositoryRequest {
        env: [("LD_PRELOAD".to_string(), "/tmp/evil.so".to_string())].into(),
        ..a_clone_of("file:///nonexistent", &repos.path().join("project"))
    };

    // When
    let refused = supervisor.client().await.clone_repository(request).await;

    // Then — the same environment policy as a session spawn
    assert!(
        refused.is_err(),
        "an environment key outside allowed_env_keys is refused"
    );
}

/// A `git` that announces some progress in the directory it was told to clone into, then waits.
fn a_git_that_never_finishes(bin: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let git = bin.join("git");
    std::fs::write(
        &git,
        "#!/bin/sh\nfor last; do :; done\nmkdir -p \"$last\"\n\
         echo 'Receiving objects:   1% (1/100)' >&2\nexec sleep 600\n",
    )
    .unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    git
}

#[tokio::test]
async fn stopping_a_supervised_clone_ends_git_and_leaves_no_destination() {
    // Given a clone that is running: git has said something, and its directory exists
    let bin = tempfile::tempdir().unwrap();
    let git = a_git_that_never_finishes(bin.path());
    let repos = tempfile::tempdir().unwrap();
    let destination = repos.path().join("project");
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(&git)
        .with_first_on_path(bin.path())
        .start()
        .await;
    let client = supervisor.client().await;
    let mut events = client
        .clone_repository(a_clone_of("file:///nowhere", &destination))
        .await
        .expect("the clone starts");
    let mut pid = 0;
    loop {
        let event = tokio::time::timeout(Duration::from_secs(30), events.recv())
            .await
            .expect("an event within 30 seconds")
            .expect("the stream is open")
            .expect("a clone event");
        match event.event.expect("a variant") {
            Event::Started(started) => pid = started.pid,
            Event::Progress(_) => break,
            Event::Exited(exited) => panic!("the clone ended on its own: {exited:?}"),
        }
    }

    // When it is stopped
    client
        .stop_session(pid)
        .await
        .expect("the stop is accepted");

    // Then git ended without succeeding, and nothing is left where it was cloning
    let exited = loop {
        let event = tokio::time::timeout(Duration::from_secs(30), events.recv())
            .await
            .expect("the clone ends within 30 seconds of being stopped")
            .expect("the stream says how git exited")
            .expect("a clone event");
        if let Some(Event::Exited(exited)) = event.event {
            break exited;
        }
    };
    assert_ne!(
        exited.code,
        Some(0),
        "a stopped clone is not a success: {exited:?}"
    );
    assert!(!destination.exists(), "no destination");
    assert_eq!(
        std::fs::read_dir(repos.path()).unwrap().count(),
        0,
        "no temporary directory left beside it"
    );
}
