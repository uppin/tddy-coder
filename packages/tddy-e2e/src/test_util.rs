//! Test utilities for E2E tests.

use std::path::PathBuf;

use tddy_core::{ActivityEntry, AppMode, PresenterView};

/// Create a temp directory with an initialized git repo (init, commit, origin/master).
/// Required for workflow steps that create worktrees (e.g. acceptance-tests).
pub fn temp_dir_with_git_repo(label: &str) -> PathBuf {
    let base = std::env::temp_dir()
        .join(format!("tddy-e2e-{}-{}-{}", label, std::process::id(), uuid::Uuid::new_v4()));
    let _ = std::fs::remove_dir_all(&base);
    let output_dir = base.join("repo");
    std::fs::create_dir_all(&output_dir).expect("create repo dir");

    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&output_dir)
            .output()
            .expect("git command");
        assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
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

    output_dir
}

/// Minimal PresenterView for tests (no-op).
pub struct NoopView;

impl PresenterView for NoopView {
    fn on_mode_changed(&mut self, _mode: &AppMode) {}
    fn on_activity_logged(&mut self, _entry: &ActivityEntry, _activity_log_len: usize) {}
    fn on_goal_started(&mut self, _goal: &str) {}
    fn on_state_changed(&mut self, _from: &str, _to: &str) {}
    fn on_workflow_complete(
        &mut self,
        _result: &Result<tddy_core::WorkflowCompletePayload, String>,
    ) {
    }
    fn on_agent_output(&mut self, _text: &str) {}
    fn on_inbox_changed(&mut self, _inbox: &[String]) {}
}
