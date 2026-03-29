//! Shared test setup. Writes a minimal YAML config under [`std::env::temp_dir`] (Unix: `TMPDIR`)
//! with `tddy_data_dir` pointing at a process-specific directory, and applies the same path via
//! [`tddy_core::output::set_tddy_data_dir_override`] so in-process code matches the production
//! opt-in YAML shape without writing session trees to `~/.tddy`.
//! Include via `mod common;` in each integration test file.

#![allow(dead_code)]

use ctor::ctor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Same path the ctor assigns to `tddy_data_dir` / the runtime override.
pub fn test_tddy_data_dir_path() -> PathBuf {
    std::env::temp_dir().join(format!("tddy-coder-test-sessions-{}", std::process::id()))
}

static ISOLATED_SESSION_SEQ: AtomicU64 = AtomicU64::new(0);

/// `{base}/sessions/<uuid-v7>/` under a fresh temp prefix — for `Presenter::start_workflow` when
/// tests would otherwise pass `session_dir: None` and allocate via global session base.
pub fn isolated_presenter_session_dir(label: &str) -> PathBuf {
    let n = ISOLATED_SESSION_SEQ.fetch_add(1, Ordering::SeqCst);
    let base = std::env::temp_dir().join(format!(
        "tddy-coder-presenter-{}-{}-{}",
        label,
        std::process::id(),
        n
    ));
    std::fs::create_dir_all(&base).expect("create isolated sessions base");
    tddy_core::output::create_session_dir_in(&base).expect("create_session_dir_in")
}

#[ctor]
fn apply_test_tddy_data_dir_config() {
    let dir = test_tddy_data_dir_path();
    let _ = std::fs::create_dir_all(&dir);
    let cfg_path = dir.join("coder-test-harness-config.yaml");
    let path_for_yaml = dir
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let yaml = format!(
        "# Test harness — same field as production coder-config.yaml (`tddy_data_dir` under TMPDIR).\ntddy_data_dir: \"{path_for_yaml}\"\n"
    );
    std::fs::write(&cfg_path, yaml).expect("write test harness coder config");
    tddy_core::output::set_tddy_data_dir_override(Some(dir));
}

/// Create a temp directory with a git repo (init, commit, origin/master).
/// Returns (output_dir, session_dir) where output_dir = base/repo (repo root) and session_dir = base/plan.
/// Session dir is next to repo, not inside it, so session artifacts stay separate from the repo.
/// Use output_dir for start_workflow when the workflow will run acceptance-tests (worktree creation).
pub fn temp_dir_with_git_repo(label: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("tddy-cli-{}-{}", label, std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let output_dir = base.join("repo");
    std::fs::create_dir_all(&output_dir).expect("create repo dir");

    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&output_dir)
            .output()
            .expect("git command");
    };
    run(&["init"]);
    run(&["config", "user.email", "test@test.com"]);
    run(&["config", "user.name", "Test"]);
    std::fs::write(output_dir.join("README"), "initial").expect("write README");
    run(&["add", "README"]);
    run(&["commit", "-m", "initial"]);
    run(&["branch", "-M", "master"]);
    run(&["remote", "add", "origin", output_dir.to_str().unwrap()]);
    run(&["push", "-u", "origin", "master"]);

    let session_dir = base.join("plan");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    (output_dir, session_dir)
}

/// Write a minimal changeset.yaml with Planned state, branch/worktree suggestions, and repo_path.
/// repo_path is the repo root (required for worktree creation when running acceptance-tests).
pub fn write_changeset_for_session(
    session_dir: &std::path::Path,
    session_id: &str,
    repo_path: &std::path::Path,
) {
    let repo_path_str = repo_path.canonicalize().unwrap_or(repo_path.to_path_buf());
    let repo_path_str = repo_path_str.display().to_string();
    let changeset = format!(
        r#"version: 1
models: {{}}
sessions:
  - id: "{}"
    agent: claude
    tag: plan
    created_at: "2026-03-07T10:00:00Z"
state:
  current: Planned
  updated_at: "2026-03-07T10:00:00Z"
  history: []
artifacts: {{}}
branch_suggestion: feature/test
worktree_suggestion: feature-test
repo_path: "{}"
"#,
        session_id,
        repo_path_str.replace('\\', "\\\\").replace('"', "\\\"")
    );
    std::fs::write(session_dir.join("changeset.yaml"), changeset).expect("write changeset");
}
