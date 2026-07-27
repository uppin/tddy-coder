//! Acceptance: repointing a planned node that is stranded behind a base branch which no longer exists.
//!
//! `repoint_planned_pr_node` now takes an explicit `target_base_branch` — the branch the operator's
//! "Repoint to <target>" control named — and applies one rule: **retain exactly the parents that own
//! that branch, drop the rest**. A target no parent owns therefore means "detach", and the node's base
//! collapses to the stack default. `None` keeps the original drop-merged-parents behaviour for callers
//! that do not name a target.
//!
//! This is what makes the reported case recoverable. A predecessor whose PR merged on GitHub and whose
//! branch was deleted is still recorded as `phase: "open"` in the plan (the orchestrator agent writes
//! that field), so the merged-parents rule could never drop it. And the node behind it was never
//! started, so it owns no branch — which used to be a hard error (`node '<id>' has no branch to
//! repoint`) on the one node the recovery exists for. Such a node is now a plan-only repoint: no
//! rebase, no force-push, no PR re-target.
//!
//! `repo_root` is a bare tempdir here, so `local_branch_exists` is false and the git rebase is
//! deterministically skipped — the assertions are about `Changeset.stack` and about which GitHub calls
//! were made.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § Repointing a dead-end planned PR (D18, D19).
//! Changeset: docs/dev/changesets.md (2026-07-26, pr-stack-repoint-dead-end).

use std::path::Path;
use std::sync::Mutex;

use tddy_core::changeset::{
    read_changeset, write_changeset, Changeset, GithubPrStatus, Stack, StackNode,
};
use tddy_workflow_recipes::orchestrate_pr_stack::github::{
    GithubPrApi, PrLookupOutcome, PrRef, PrState, PrView,
};
use tddy_workflow_recipes::pr_stack::repoint_planned_pr_node;

const DEFAULT_BRANCH: &str = "master";
/// The merged predecessor's branch — deleted from `origin` once its PR landed.
const DELETED_BASE_BRANCH: &str = "feature/attach-docs/attach-proto";
/// A second predecessor's branch, still open and still on `origin`.
const LIVE_BASE_BRANCH: &str = "feature/attach-docs/attach-store";

// --- a fake GitHub PR API that records every call it is asked to make --------

struct FakeGithub {
    /// Every head branch a PR was looked up for — so a test can assert GitHub was never consulted.
    looked_up: Mutex<Vec<String>>,
    patched_bases: Mutex<Vec<(u64, String)>>,
}

impl FakeGithub {
    fn new() -> Self {
        Self {
            looked_up: Mutex::new(Vec::new()),
            patched_bases: Mutex::new(Vec::new()),
        }
    }

    fn looked_up(&self) -> Vec<String> {
        self.looked_up.lock().unwrap().clone()
    }

    fn patched_bases(&self) -> Vec<(u64, String)> {
        self.patched_bases.lock().unwrap().clone()
    }
}

impl GithubPrApi for FakeGithub {
    fn get_open_pr(&self, head_branch: &str) -> Result<Option<PrRef>, tddy_core::WorkflowError> {
        self.looked_up.lock().unwrap().push(head_branch.to_string());
        Ok(Some(PrRef {
            number: 42,
            head_sha: "sha42".to_string(),
            base_branch: DELETED_BASE_BRANCH.to_string(),
            url: "https://github.com/acme/repo/pull/42".to_string(),
        }))
    }
    fn get_pr_by_head(&self, _head_branch: &str) -> PrLookupOutcome {
        PrLookupOutcome::Found(PrView {
            number: 42,
            url: "https://github.com/acme/repo/pull/42".to_string(),
            state: PrState::Open,
        })
    }
    fn merge_pr(&self, _number: u64) -> Result<String, tddy_core::WorkflowError> {
        Ok("merge-sha".to_string())
    }
    fn patch_pr_base(&self, number: u64, new_base: &str) -> Result<(), tddy_core::WorkflowError> {
        self.patched_bases
            .lock()
            .unwrap()
            .push((number, new_base.to_string()));
        Ok(())
    }
    fn create_pr(
        &self,
        _head: &str,
        _base: &str,
        _title: &str,
        _body: &str,
    ) -> Result<u64, tddy_core::WorkflowError> {
        Ok(99)
    }
    fn disable_auto_merge(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }
    fn close_pr(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }
}

// --- builders ---------------------------------------------------------------

/// A node that owns a branch and whose PR is recorded as still open.
fn an_open_node(node_id: &str, branch: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: node_id.to_string(),
        description: String::new(),
        branch_suggestion: None,
        branch: Some(branch.to_string()),
        session_id: Some(format!("session-for-{node_id}")),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        pr_status: Some(GithubPrStatus {
            phase: "open".to_string(),
            url: Some(format!("https://github.com/acme/repo/pull/{node_id}")),
            error: None,
        }),
        child_state: None,
        internal_status: None,
    }
}

/// A node whose PR is recorded as merged.
fn a_merged_node(node_id: &str, branch: &str) -> StackNode {
    StackNode {
        pr_status: Some(GithubPrStatus {
            phase: "merged".to_string(),
            url: None,
            error: None,
        }),
        ..an_open_node(node_id, branch, &[])
    }
}

/// A planned node that was never started: it holds a suggested branch name and owns no branch.
fn a_planned_node(node_id: &str, parents: &[&str]) -> StackNode {
    StackNode {
        branch: None,
        session_id: None,
        pr_status: None,
        branch_suggestion: Some(format!("feature/attach-docs/{node_id}")),
        ..an_open_node(node_id, "unused", parents)
    }
}

fn write_stack(dir: &Path, nodes: Vec<StackNode>) {
    let cs = Changeset {
        stack: Some(Stack { version: 1, nodes }),
        ..Changeset::default()
    };
    write_changeset(dir, &cs).unwrap();
}

fn parents_of(dir: &Path, node_id: &str) -> Vec<String> {
    read_changeset(dir)
        .unwrap()
        .stack
        .unwrap()
        .node(node_id)
        .unwrap()
        .parents
        .clone()
}

// --- tests ------------------------------------------------------------------

#[test]
fn repointing_a_branchless_node_to_the_default_branch_drops_its_stranded_parent() {
    // Given — n1's PR merged on GitHub and its branch was deleted, but the plan still says "open";
    // n2 was never started, so it owns no branch
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", DELETED_BASE_BRANCH, &[]),
            a_planned_node("n2", &["n1"]),
        ],
    );
    let gh = FakeGithub::new();

    // When — the operator repoints n2 onto the default branch, which no parent owns
    repoint_planned_pr_node(dir, dir, "n2", DEFAULT_BRANCH, Some(DEFAULT_BRANCH), &gh)
        .expect("repointing a branchless node should succeed");

    // Then — the stranded parent is gone, so n2's base collapses to the default branch
    assert_eq!(parents_of(dir, "n2"), Vec::<String>::new());
}

#[test]
fn repointing_a_branchless_node_makes_no_github_call() {
    // Given — the same stranded node, which owns no branch
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", DELETED_BASE_BRANCH, &[]),
            a_planned_node("n2", &["n1"]),
        ],
    );
    let gh = FakeGithub::new();

    // When
    repoint_planned_pr_node(dir, dir, "n2", DEFAULT_BRANCH, Some(DEFAULT_BRANCH), &gh)
        .expect("repointing a branchless node should succeed");

    // Then — a node with no branch has no PR of its own, so there is nothing to look up and nothing to
    // re-target: dropping the dead parent is the whole repoint
    assert_eq!(gh.looked_up(), Vec::<String>::new());
    assert_eq!(gh.patched_bases(), Vec::<(u64, String)>::new());
}

#[test]
fn repointing_retains_the_parent_that_owns_the_target_base_branch() {
    // Given — n3 depends on n1 (branch deleted from origin, still recorded open) and n2 (open, pushed)
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", DELETED_BASE_BRANCH, &[]),
            an_open_node("n2", LIVE_BASE_BRANCH, &[]),
            a_planned_node("n3", &["n1", "n2"]),
        ],
    );
    let gh = FakeGithub::new();

    // When — the operator repoints onto the surviving predecessor's branch
    repoint_planned_pr_node(dir, dir, "n3", DEFAULT_BRANCH, Some(LIVE_BASE_BRANCH), &gh)
        .expect("repointing onto a surviving parent should succeed");

    // Then — retain is the rule: only the parent that owns the target survives
    assert_eq!(parents_of(dir, "n3"), vec!["n2".to_string()]);
}

#[test]
fn repointing_collapses_a_multi_parent_node_onto_the_single_target_parent() {
    // Given — n4 stacks on three predecessors, two of which are perfectly healthy: n2 owns the target
    // branch and n3 owns a different live one
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            a_merged_node("n1", DELETED_BASE_BRANCH),
            an_open_node("n2", LIVE_BASE_BRANCH, &[]),
            an_open_node("n3", "feature/attach-docs/attach-third", &[]),
            a_planned_node("n4", &["n1", "n2", "n3"]),
        ],
    );
    let gh = FakeGithub::new();

    // When
    repoint_planned_pr_node(dir, dir, "n4", DEFAULT_BRANCH, Some(LIVE_BASE_BRANCH), &gh)
        .expect("repointing a multi-parent node should succeed");

    // Then — repointing is a decision to stack on one predecessor, so n3's healthy edge is dropped
    // too and the node comes out single-parent. This is the intended collapse, not a casualty of the
    // target being a single branch name.
    assert_eq!(parents_of(dir, "n4"), vec!["n2".to_string()]);
}

#[test]
fn repointing_drops_a_parent_whose_pull_request_is_not_recorded_as_merged() {
    // Given — n1's plan status is "open" (the orchestrator agent never ran an assess pass) even though
    // its branch is gone; n2 owns a branch, so the git/PR half of the repoint also runs
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", DELETED_BASE_BRANCH, &[]),
            an_open_node("n2", "feature/attach-docs/attach-start", &["n1"]),
        ],
    );
    let gh = FakeGithub::new();

    // When
    repoint_planned_pr_node(dir, dir, "n2", DEFAULT_BRANCH, Some(DEFAULT_BRANCH), &gh)
        .expect("repointing should succeed");

    // Then — a stale "open" phase no longer protects a dead parent
    assert_eq!(parents_of(dir, "n2"), Vec::<String>::new());
}

#[test]
fn repointing_onto_an_explicit_target_re_targets_the_nodes_own_open_pull_request() {
    // Given — n2 owns a branch and has an open PR of its own, so the git/PR half of the repoint runs
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", DELETED_BASE_BRANCH, &[]),
            an_open_node("n2", "feature/attach-docs/attach-start", &["n1"]),
        ],
    );
    let gh = FakeGithub::new();

    // When — the operator names the default branch as the target (rather than the `None` mode, which
    // `pr_stack_repoint_acceptance.rs` covers)
    repoint_planned_pr_node(dir, dir, "n2", DEFAULT_BRANCH, Some(DEFAULT_BRANCH), &gh)
        .expect("repointing should succeed");

    // Then — the PR on GitHub follows the new base, so it no longer diffs against a dead branch
    assert_eq!(gh.patched_bases(), vec![(42, DEFAULT_BRANCH.to_string())]);
}

#[test]
fn repointing_without_a_target_drops_merged_parents_only() {
    // Given — n2 depends on n1 (merged) and n3 (still open)
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            a_merged_node("n1", DELETED_BASE_BRANCH),
            an_open_node("n3", LIVE_BASE_BRANCH, &[]),
            an_open_node("n2", "feature/attach-docs/attach-start", &["n1", "n3"]),
        ],
    );
    let gh = FakeGithub::new();

    // When — no target is named, so the original rule applies
    repoint_planned_pr_node(dir, dir, "n2", DEFAULT_BRANCH, None, &gh)
        .expect("repointing should succeed");

    // Then — the merged parent is dropped and the open one is kept, exactly as before
    assert_eq!(parents_of(dir, "n2"), vec!["n3".to_string()]);
}
