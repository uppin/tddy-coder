//! Acceptance: the clone helper clones into a partial sibling and renames it into place, removes
//! the partial directory when git fails, and refuses a destination that already exists
//! (`#agent-tools` 32/32).
//!
//! The helper runs for real against a local bare repository over `file://`, as the account running
//! the tests — no network, no supervisor.
//!
//! Feature docs: docs/ft/supervisor/1-WIP/PRD-2026-11-07-supervisor-agent-agnostic.md
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// How long one helper run may take before the test gives up on it.
const A_CLONE_THAT_HANGS: Duration = Duration::from_secs(60);

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?}");
}

/// A bare repository holding one commit with `README`, as a `file://` URL.
fn a_bare_repository(dir: &Path) -> String {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).expect("create the work tree");
    git(&work, &["init", "-q"]);
    std::fs::write(work.join("README"), "hello\n").expect("write README");
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "seed"]);
    git(dir, &["clone", "-q", "--bare", "work", "origin.git"]);
    format!("file://{}", dir.join("origin.git").display())
}

/// The `git` this host runs, resolved to an absolute path as the daemon would hand it over.
fn the_git_on_path() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").expect("a PATH"))
        .map(|dir| dir.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("git on PATH")
}

/// Run the helper to completion, within the deadline.
fn the_helper_cloning(url: &str, destination: &Path) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tddy-git-clone"))
        .arg("--git")
        .arg(the_git_on_path())
        .arg("--")
        .arg(url)
        .arg(destination)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start the clone helper");
    let deadline = Instant::now() + A_CLONE_THAT_HANGS;
    while child.try_wait().expect("poll the clone helper").is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("the clone helper did not finish within {A_CLONE_THAT_HANGS:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child
        .wait_with_output()
        .expect("collect the helper's output")
}

fn entries_of(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read the repos directory")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_clone_goes_into_a_partial_sibling_and_is_renamed_into_place() {
    // Given
    let source = tempfile::tempdir().expect("source dir");
    let url = a_bare_repository(source.path());
    let repos = tempfile::tempdir().expect("repos dir");
    let destination = repos.path().join("project");

    // When
    let output = the_helper_cloning(&url, &destination);

    // Then git cloned into `project.partial-…`, and only the finished clone is left, in place
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "the helper succeeded: {stderr}");
    assert!(
        stderr.contains("project.partial-"),
        "git was pointed at a partial sibling, not the destination: {stderr}"
    );
    assert_eq!(
        (
            entries_of(repos.path()),
            std::fs::read_to_string(destination.join("README")).ok(),
        ),
        (vec!["project".to_string()], Some("hello\n".to_string()))
    );
}

#[test]
fn a_failed_clone_removes_its_partial_directory_and_carries_gits_words() {
    // Given
    let repos = tempfile::tempdir().expect("repos dir");
    let destination = repos.path().join("project");

    // When
    let output = the_helper_cloning("file:///nonexistent/tddy-no-such-repository", &destination);

    // Then nothing is left behind, and git said why
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the helper failed: {stderr}");
    assert!(
        stderr.contains("tddy-no-such-repository"),
        "git's own explanation reaches stderr: {stderr}"
    );
    assert_eq!(entries_of(repos.path()), Vec::<String>::new());
}

#[test]
fn a_destination_that_already_exists_is_refused_and_left_alone() {
    // Given a destination that is already there
    let source = tempfile::tempdir().expect("source dir");
    let url = a_bare_repository(source.path());
    let repos = tempfile::tempdir().expect("repos dir");
    let destination = repos.path().join("project");
    std::fs::create_dir_all(&destination).expect("create the destination");
    std::fs::write(destination.join("mine.txt"), "keep me\n").expect("write a marker");

    // When
    let output = the_helper_cloning(&url, &destination);

    // Then it is refused, naming the destination, and nothing was touched or left beside it
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the helper refused: {stderr}");
    assert!(
        stderr.contains("already exists"),
        "the refusal says why: {stderr}"
    );
    assert_eq!(
        (
            entries_of(repos.path()),
            std::fs::read_to_string(destination.join("mine.txt")).ok(),
        ),
        (vec!["project".to_string()], Some("keep me\n".to_string()))
    );
}
