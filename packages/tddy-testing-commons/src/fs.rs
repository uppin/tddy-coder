//! Shared filesystem helpers for tests.

use std::path::PathBuf;
use tddy_core::{write_session_metadata, SessionMetadata};

/// Create a unique temp directory with `label` in the name.
///
/// The directory is removed and re-created fresh. Returns the path.
pub fn temp_session_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tddy-test-{}-{}", label, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp session dir");
    dir
}

/// Create a unique temp directory with an initialized git repository.
///
/// Runs `git init`, creates an initial commit on `master`, and adds `origin` pointing to itself.
/// Required for workflow steps that create worktrees.
pub fn temp_dir_with_git_repo(label: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "tddy-test-git-{}-{}-{}",
        label,
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let _ = std::fs::remove_dir_all(&base);
    let repo = base.join("repo");
    std::fs::create_dir_all(&repo).expect("create repo dir");

    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .expect("git command");
        assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
    };
    run(&["init"]);
    run(&["config", "user.email", "test@test.com"]);
    run(&["config", "user.name", "Test"]);
    std::fs::write(repo.join("README"), "initial").expect("write README");
    run(&["add", "README"]);
    run(&["commit", "-m", "initial"]);
    run(&["branch", "-M", "master"]);
    run(&[
        "remote",
        "add",
        "origin",
        repo.to_str().expect("utf-8 path"),
    ]);
    run(&["push", "-u", "origin", "master"]);

    repo
}

/// Write a [`SessionMetadata`] YAML to `{session_dir}/.session.yaml`.
///
/// Convenience wrapper over [`tddy_core::write_session_metadata`] for test setup.
pub fn write_session_yaml(session_dir: &std::path::Path, metadata: &SessionMetadata) {
    write_session_metadata(session_dir, metadata)
        .expect("write_session_metadata must succeed in test setup");
}
