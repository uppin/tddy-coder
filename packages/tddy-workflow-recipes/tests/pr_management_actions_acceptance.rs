//! PRD acceptance: the PR-management actions the `orchestrate` tools delegate to.
//!
//! These are the on-demand replacements for the old autonomous merge/repoint loop: the agent
//! calls a `tddy-tools` `pr_*` tool, which resolves the node in the orchestrator changeset and
//! calls one of these actions. Each action reuses `GithubPrApi` / git and records the outcome
//! back onto the node's `pr_status`.
//!
//! PRD: docs/ft/coder/pr-stacking.md § PR-management tools.

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use tddy_core::changeset::{
    read_changeset, write_changeset, Changeset, GithubPrStatus, Stack, StackNode,
};
use tddy_workflow_recipes::orchestrate_pr_stack::github::{GithubPrApi, PrRef};
use tddy_workflow_recipes::orchestrate_pr_stack::{
    pr_close_action, pr_merge_action, pr_resolve_conflicts_action,
};

// ---------------------------------------------------------------------------
// A recording GitHub API fake: implements the whole trait, records merge/close calls.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RecordingGithub {
    merged: Mutex<Vec<u64>>,
    closed: Mutex<Vec<u64>>,
}

impl GithubPrApi for RecordingGithub {
    fn get_open_pr(&self, _head_branch: &str) -> Result<Option<PrRef>, tddy_core::WorkflowError> {
        Ok(None)
    }
    fn merge_pr(&self, number: u64) -> Result<String, tddy_core::WorkflowError> {
        self.merged.lock().unwrap().push(number);
        Ok("merge-sha-9f2".to_string())
    }
    fn patch_pr_base(&self, _number: u64, _new_base: &str) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }
    fn create_pr(
        &self,
        _head: &str,
        _base: &str,
        _title: &str,
        _body: &str,
    ) -> Result<u64, tddy_core::WorkflowError> {
        Ok(1)
    }
    fn disable_auto_merge(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }
    fn close_pr(&self, number: u64) -> Result<(), tddy_core::WorkflowError> {
        self.closed.lock().unwrap().push(number);
        Ok(())
    }
}

fn scratch(label: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("tddy-pr-actions-{}-{}", label, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn an_orchestrator_dir_with_open_pr_node(label: &str) -> std::path::PathBuf {
    let dir = scratch(label);
    let cs = Changeset {
        stack: Some(Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".into(),
                title: "Add token store".into(),
                description: String::new(),
                branch_suggestion: Some("feature/auth/token-store".into()),
                branch: Some("feature/auth/token-store".into()),
                session_id: Some("sid-n1".into()),
                parents: vec![],
                pr_status: Some(GithubPrStatus {
                    phase: "open".into(),
                    url: None,
                    error: None,
                }),
                child_state: None,
                internal_status: None,
            }],
        }),
        ..Changeset::default()
    };
    write_changeset(&dir, &cs).unwrap();
    dir
}

fn node_phase(dir: &Path, node_id: &str) -> String {
    read_changeset(dir)
        .unwrap()
        .stack
        .unwrap()
        .node(node_id)
        .unwrap()
        .pr_status
        .clone()
        .unwrap()
        .phase
}

#[test]
fn merge_action_merges_the_pr_and_marks_the_node_merged() {
    // Given
    let dir = an_orchestrator_dir_with_open_pr_node("merge");
    let api = RecordingGithub::default();

    // When
    let sha = pr_merge_action(&dir, &api, "n1", 42).unwrap();

    // Then
    assert_eq!(sha, "merge-sha-9f2");
    assert_eq!(api.merged.lock().unwrap().as_slice(), &[42]);
    assert_eq!(node_phase(&dir, "n1"), "merged");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn close_action_closes_the_pr_and_marks_the_node_closed() {
    // Given
    let dir = an_orchestrator_dir_with_open_pr_node("close");
    let api = RecordingGithub::default();

    // When
    pr_close_action(&dir, &api, "n1", 42).unwrap();

    // Then
    assert_eq!(api.closed.lock().unwrap().as_slice(), &[42]);
    assert_eq!(node_phase(&dir, "n1"), "closed");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resolve_conflicts_action_returns_the_conflicting_paths() {
    // Given — a git repo where merging `base-branch` into the current branch conflicts on file.txt
    let repo = scratch("conflicts");
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {:?} failed", args);
    };
    let write = |name: &str, contents: &str| {
        std::fs::write(repo.join(name), contents).unwrap();
    };

    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "t@example.com"]);
    git(&["config", "user.name", "Tester"]);
    write("file.txt", "original\n");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "base"]);

    git(&["checkout", "-q", "-b", "base-branch"]);
    write("file.txt", "changed on base\n");
    git(&["commit", "-qam", "base edit"]);

    git(&["checkout", "-q", "main"]);
    git(&["checkout", "-q", "-b", "feature"]);
    write("file.txt", "changed on feature\n");
    git(&["commit", "-qam", "feature edit"]);

    // When — while on `feature`, resolve-conflicts against `base-branch`
    let conflicted = pr_resolve_conflicts_action(&repo, "base-branch").unwrap();

    // Then
    assert_eq!(conflicted, vec!["file.txt".to_string()]);
    let _ = std::fs::remove_dir_all(&repo);
}
