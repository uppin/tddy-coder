//! Acceptance: a clone reports git's progress, lands at its destination only when git succeeds, and
//! leaves nothing behind when git fails or the clone is stopped.
//!
//! Real `git` clones a generated local repository over `file://` (which, unlike a plain path, makes
//! git pack and transfer objects, so it reports progress). The stopped clone uses a fake `git` on the
//! child's `PATH`, carried in the clone's own environment. Everything runs as the current OS user.
//!
//! Feature docs: docs/ft/daemon/1-WIP/PRD-2026-10-24-start-progress-clone-progress.md
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use tddy_spawn::clone_run::{clone_with_progress, CloneHandle};
use tddy_spawn::git_progress::GitProgress;

fn the_current_os_user() -> String {
    let output = Command::new("id").arg("-un").output().unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

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

/// A repository with enough objects that git reports transfer progress.
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

fn entries_of(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_clone_reports_progress_and_lands_at_its_destination() {
    // Given
    let source = tempfile::tempdir().unwrap();
    let url = a_repository_worth_cloning(source.path());
    let repos = tempfile::tempdir().unwrap();
    let destination = repos.path().join("project");
    let mut seen: Vec<GitProgress> = Vec::new();

    // When
    clone_with_progress(
        &the_current_os_user(),
        &url,
        &destination,
        &[],
        |_| {},
        |p| seen.push(p),
    )
    .expect("the clone succeeds");

    // Then — a complete clone at the destination, nothing else beside it, and git's stages reported
    assert!(
        destination.join(".git").is_dir(),
        "a complete clone at the destination"
    );
    assert_eq!(
        entries_of(repos.path()),
        vec!["project".to_string()],
        "no temporary directory left"
    );
    assert!(
        seen.iter()
            .any(|p| p.stage == "Receiving objects" && p.objects_total > 0),
        "git's receiving stage was reported: {seen:?}"
    );
}

#[test]
fn a_failed_clone_leaves_no_destination_and_carries_gits_message() {
    // Given
    let repos = tempfile::tempdir().unwrap();
    let destination = repos.path().join("project");

    // When
    let error = clone_with_progress(
        &the_current_os_user(),
        "file:///nonexistent/tddy-no-such-repository",
        &destination,
        &[],
        |_| {},
        |_| {},
    )
    .expect_err("a clone of nothing fails");

    // Then
    let message = format!("{error:#}");
    assert!(
        message.contains("tddy-no-such-repository"),
        "the error carries git's own message: {message}"
    );
    assert!(
        entries_of(repos.path()).is_empty(),
        "nothing left behind: {:?}",
        entries_of(repos.path())
    );
}

#[test]
fn a_stopped_clone_ends_its_git_process_and_leaves_no_destination() {
    // Given — a git that reports a little progress and then takes forever
    let fake = tempfile::tempdir().unwrap();
    let git = fake.path().join("git");
    std::fs::write(
        &git,
        "#!/bin/sh\nfor last; do :; done\nmkdir -p \"$last\"\n\
         printf 'Receiving objects:   1%% (1/100)\\r' >&2\nexec sleep 60\n",
    )
    .unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        fake.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let repos = tempfile::tempdir().unwrap();
    let destination = repos.path().join("project");
    let (handles, handle) = std::sync::mpsc::channel::<CloneHandle>();
    let started = Instant::now();

    // When — the clone is stopped once it is running
    let stopper = std::thread::spawn(move || {
        let handle = handle
            .recv_timeout(Duration::from_secs(10))
            .expect("the clone started");
        std::thread::sleep(Duration::from_millis(300));
        handle.stop().expect("stop the clone");
        handle.pid()
    });
    let result = clone_with_progress(
        &the_current_os_user(),
        "https://example.invalid/slow.git",
        &destination,
        &[("PATH", path.as_str())],
        move |h| handles.send(h).unwrap(),
        |_| {},
    );
    let pid = stopper.join().unwrap();

    // Then — it ended promptly, its git is gone, and nothing is left behind
    assert!(result.is_err(), "a stopped clone is not a success");
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "it ended when stopped, took {:?}",
        started.elapsed()
    );
    assert_ne!(
        unsafe { libc::kill(pid as libc::pid_t, 0) },
        0,
        "the clone's git process {pid} is gone"
    );
    assert!(
        entries_of(repos.path()).is_empty(),
        "nothing left behind: {:?}",
        entries_of(repos.path())
    );
}
