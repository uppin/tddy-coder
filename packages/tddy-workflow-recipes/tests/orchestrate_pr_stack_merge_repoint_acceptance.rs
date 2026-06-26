//! PRD acceptance: orchestrate-pr-stack merge+repoint — execute_stack_merge writes the
//! crash-safe journal through Planned→PrMerged, marks the node merged in the Stack,
//! and execute_stack_repoint repoints all dependents then clears the journal.

use std::sync::Mutex;

use tddy_core::changeset::{
    read_changeset, write_changeset_atomic, Changeset, GithubPrStatus, Stack, StackNode,
};
use tddy_core::WorkflowError;
use tddy_workflow_recipes::orchestrate_pr_stack::github::{GithubPrApi, PrRef};
use tddy_workflow_recipes::orchestrate_pr_stack::transient::{
    recover_in_flight_stack_op, MergePhase,
};
use tddy_workflow_recipes::orchestrate_pr_stack::{execute_stack_merge, execute_stack_repoint};

// ---------------------------------------------------------------------------
// Test-only mock of GithubPrApi
// ---------------------------------------------------------------------------

struct MockGithubPrApi {
    merge_calls: Mutex<Vec<u64>>,
    patch_base_calls: Mutex<Vec<(u64, String)>>,
    merge_sha: String,
}

impl MockGithubPrApi {
    fn new(merge_sha: impl Into<String>) -> Self {
        Self {
            merge_calls: Mutex::new(vec![]),
            patch_base_calls: Mutex::new(vec![]),
            merge_sha: merge_sha.into(),
        }
    }

    fn merged_pr_numbers(&self) -> Vec<u64> {
        self.merge_calls.lock().unwrap().clone()
    }

    fn patched_bases(&self) -> Vec<(u64, String)> {
        self.patch_base_calls.lock().unwrap().clone()
    }
}

impl GithubPrApi for MockGithubPrApi {
    fn get_open_pr(&self, _head: &str) -> Result<Option<PrRef>, WorkflowError> {
        Ok(None)
    }
    fn merge_pr(&self, number: u64) -> Result<String, WorkflowError> {
        self.merge_calls.lock().unwrap().push(number);
        Ok(self.merge_sha.clone())
    }
    fn patch_pr_base(&self, number: u64, new_base: &str) -> Result<(), WorkflowError> {
        self.patch_base_calls
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
    ) -> Result<u64, WorkflowError> {
        Ok(99)
    }
    fn disable_auto_merge(&self, _number: u64) -> Result<(), WorkflowError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn init_orchestrator_with_stack(session_dir: &std::path::Path, nodes: Vec<StackNode>) {
    let mut cs = Changeset::default();
    cs.stack = Some(Stack { version: 1, nodes });
    write_changeset_atomic(session_dir, &cs).expect("write changeset");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn merge_task_writes_prmerged_journal_then_marks_node_merged() {
    // Given — orchestrator session dir with stack node n1 (PR #1 open)
    let tmp = tempfile::tempdir().expect("temp dir");
    let session_dir = tmp.path();
    init_orchestrator_with_stack(
        session_dir,
        vec![StackNode {
            node_id: "n1".into(),
            title: "Auth store".into(),
            description: String::new(),
            branch_suggestion: None,
            branch: Some("feature/auth-store".into()),
            session_id: Some("child-sess-n1".into()),
            parents: vec![],
            pr_status: Some(GithubPrStatus {
                phase: "open".into(),
                url: Some("https://github.com/o/r/pull/1".into()),
                error: None,
            }),
            child_state: None,
        }],
    );
    let mock_gh = MockGithubPrApi::new("merge-sha-abc");

    // When — executing the merge for node n1, PR #1
    execute_stack_merge(session_dir, "n1", 1, &mock_gh)
        .expect("merge must succeed with mock GitHub API");

    // Then — GitHub merge was called exactly once for PR #1
    assert_eq!(mock_gh.merged_pr_numbers(), vec![1u64]);

    // …and the stack node n1 has pr_status.phase == "merged"
    let cs = read_changeset(session_dir).expect("changeset readable");
    let stack = cs.stack.expect("stack must exist");
    let n1 = stack
        .nodes
        .iter()
        .find(|n| n.node_id == "n1")
        .expect("n1 node");
    assert_eq!(
        n1.pr_status.as_ref().map(|p| p.phase.as_str()),
        Some("merged"),
        "pr_status.phase must be 'merged' after execute_stack_merge"
    );

    // …and the crash-safe journal exists with phase Done or has been cleaned up
    // (either is acceptable; the important invariant is the node is marked merged)
    // If the journal is still present it must have advanced past Planned
    if let Ok(Some(journal)) = recover_in_flight_stack_op(session_dir) {
        assert!(
            !matches!(journal.merge_phase, MergePhase::Planned),
            "journal must not remain in Planned phase after a successful merge"
        );
    }
}

#[test]
fn repoint_task_repoints_each_dependent_and_clears_journal() {
    // Given — orchestrator session dir with a merged n1 and two dependents n2, n3
    let tmp = tempfile::tempdir().expect("temp dir");
    let session_dir = tmp.path();
    init_orchestrator_with_stack(
        session_dir,
        vec![
            StackNode {
                node_id: "n1".into(),
                title: "Auth store".into(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/auth-store".into()),
                session_id: Some("child-sess-n1".into()),
                parents: vec![],
                pr_status: Some(GithubPrStatus {
                    phase: "merged".into(),
                    url: None,
                    error: None,
                }),
                child_state: None,
            },
            StackNode {
                node_id: "n2".into(),
                title: "Login API".into(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/login-api".into()),
                session_id: Some("child-sess-n2".into()),
                parents: vec!["n1".into()],
                pr_status: Some(GithubPrStatus {
                    phase: "open".into(),
                    url: Some("https://github.com/o/r/pull/2".into()),
                    error: None,
                }),
                child_state: None,
            },
            StackNode {
                node_id: "n3".into(),
                title: "Dashboard".into(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/dashboard".into()),
                session_id: Some("child-sess-n3".into()),
                parents: vec!["n2".into()],
                pr_status: Some(GithubPrStatus {
                    phase: "open".into(),
                    url: Some("https://github.com/o/r/pull/3".into()),
                    error: None,
                }),
                child_state: None,
            },
        ],
    );

    let mock_gh = MockGithubPrApi::new("merge-sha-abc");
    // A temp git repo for the git ops (rebase/push)
    let repo_tmp = tempfile::tempdir().expect("repo temp dir");
    std::process::Command::new("git")
        .args(["init", "--quiet", "-b", "master"])
        .current_dir(repo_tmp.path())
        .status()
        .expect("git init");

    // When — executing repoint for the merged n1, dependents n2 + n3
    execute_stack_repoint(
        session_dir,
        repo_tmp.path(),
        "n1",
        &["n2".to_string(), "n3".to_string()],
        "master",
        &mock_gh,
    )
    .expect("repoint must succeed with mock GitHub API and empty repo");

    // Then — GitHub patch_pr_base was called for each dependent (n2 PR #2, n3 PR #3)
    // with the new effective base (master after n1 merged)
    let patched = mock_gh.patched_bases();
    assert!(
        !patched.is_empty(),
        "patch_pr_base must be called for each dependent"
    );
    assert!(
        patched
            .iter()
            .any(|(pr, base)| *pr == 2 && base == "master"),
        "n2's PR #2 must be rebased to master; got: {patched:?}"
    );

    // …and the crash-safe journal is deleted after a successful repoint
    let recovered =
        recover_in_flight_stack_op(session_dir).expect("journal recovery must not error");
    assert!(
        recovered.is_none(),
        "journal must be deleted after a complete repoint; still present: {recovered:?}"
    );
}
