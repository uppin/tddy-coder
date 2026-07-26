//! Acceptance: repointing a single planned node after a predecessor merged.
//!
//! `repoint_planned_pr_node` drops merged parents from the node, rebases the node's local branch
//! onto the effective base (skipped when the branch isn't local), and re-targets the open GitHub
//! PR's base — the same primitives `bridge::execute_stack_repoint` uses, applied to one node so
//! the web Repoint control and the agent repoint converge.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § capability 4.

use std::path::Path;
use std::sync::Mutex;

use tddy_core::changeset::{
    read_changeset, write_changeset, Changeset, GithubPrStatus, Stack, StackNode,
};
use tddy_workflow_recipes::orchestrate_pr_stack::github::{
    GithubPrApi, PrLookupOutcome, PrRef, PrState, PrView,
};
use tddy_workflow_recipes::pr_stack::repoint_planned_pr_node;

// --- a fake GitHub PR API that records base re-targeting ---------------------

struct FakeGithub {
    patched_bases: Mutex<Vec<(u64, String)>>,
}

impl FakeGithub {
    fn new() -> Self {
        Self {
            patched_bases: Mutex::new(Vec::new()),
        }
    }
}

impl GithubPrApi for FakeGithub {
    fn get_open_pr(&self, _head_branch: &str) -> Result<Option<PrRef>, tddy_core::WorkflowError> {
        Ok(Some(PrRef {
            number: 42,
            head_sha: "sha42".to_string(),
            base_branch: "feature/x/n1".to_string(),
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

fn a_node(node_id: &str, branch: &str, parents: &[&str]) -> StackNode {
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

fn a_merged_node(node_id: &str, branch: &str) -> StackNode {
    StackNode {
        pr_status: Some(GithubPrStatus {
            phase: "merged".to_string(),
            url: None,
            error: None,
        }),
        ..a_node(node_id, branch, &[])
    }
}

fn write_stack(dir: &Path, nodes: Vec<StackNode>) {
    let cs = Changeset {
        stack: Some(Stack { version: 1, nodes }),
        ..Changeset::default()
    };
    write_changeset(dir, &cs).unwrap();
}

// --- tests ------------------------------------------------------------------

#[test]
fn repoint_planned_pr_node_drops_a_merged_parent_from_the_node_parents() {
    // Given — n2 depends on n1 (merged) and n3 (still open); its branch is remote-only here so the
    // git rebase is skipped and the test stays deterministic.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            a_merged_node("n1", "feature/x/n1"),
            a_node("n3", "feature/x/n3", &[]),
            a_node("n2", "feature/x/n2", &["n1", "n3"]),
        ],
    );
    let gh = FakeGithub::new();

    // When
    repoint_planned_pr_node(dir, dir, "n2", "master", &gh).expect("repoint should succeed");

    // Then — the merged parent n1 is gone; the open parent n3 remains
    let loaded = read_changeset(dir).unwrap().stack.unwrap();
    assert_eq!(loaded.node("n2").unwrap().parents, vec!["n3".to_string()]);
}

#[test]
fn repoint_planned_pr_node_retargets_the_open_pr_base_to_the_effective_base() {
    // Given — n2's only parent n1 has merged, so its effective base becomes the stack default
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            a_merged_node("n1", "feature/x/n1"),
            a_node("n2", "feature/x/n2", &["n1"]),
        ],
    );
    let gh = FakeGithub::new();

    // When
    repoint_planned_pr_node(dir, dir, "n2", "master", &gh).expect("repoint should succeed");

    // Then — the open PR (#42) is re-based onto the stack default branch on GitHub
    assert_eq!(
        gh.patched_bases.lock().unwrap().as_slice(),
        &[(42, "master".to_string())]
    );
}
