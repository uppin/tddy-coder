//! Acceptance: what a move does when git refuses to follow it.
//!
//! `set_stack_node_parents` writes the new DAG and then makes reality match it: rebase the node's
//! branch onto its new effective base, force-push it, re-target its pull request. The rebase is the
//! half that can fail against the developer's own repository, and every other PR-stack acceptance file
//! passes a bare tempdir as `repo_root` so `local_branch_exists` is false and the git half is skipped
//! entirely. This file is the one that does not: it builds a real repository whose branches conflict,
//! so the failure arm — record `pr_status.phase = "error"` with the git message, and do *not* re-target
//! the pull request — is exercised rather than assumed.
//!
//! Re-targeting a PR onto a base its branch does not sit on would misdescribe reality to every
//! reviewer, so the PR is deliberately left pointing where it did and the branch is left for a human.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-07-30-pr-stack-full-control.md § pr_set_parents.
//! Changeset: docs/dev/1-WIP/2026-07-30-pr-stack-full-control.md.

mod common;

use std::path::Path;
use std::process::Command;

use common::{a_stack_github, an_open_node, assert_rejected, parents_of, stack_of, write_stack};
use tddy_workflow_recipes::pr_stack::set_stack_node_parents;

const BRANCH_N2: &str = "feature/stack/n2";
const BRANCH_N3: &str = "feature/stack/n3";
const PR_OF_N2: u64 = 7;
const PR_OF_N3: u64 = 42;
const DEFAULT_BRANCH: &str = "master";

/// A repository where n3's commit cannot be replayed onto n2's: both branches fork from the same
/// commit and rewrite the same line of the same file, so the rebase conflicts for the one reason git
/// always conflicts, on every platform and every git version.
fn a_repo_whose_branches_conflict(repo: &Path) {
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .expect("git must be on PATH");
        assert!(status.success(), "git {args:?} failed");
    };
    let write = |contents: &str| std::fs::write(repo.join("token_store.rs"), contents).unwrap();

    git(&["init", "-q", "-b", DEFAULT_BRANCH]);
    git(&["config", "user.email", "operator@example.com"]);
    git(&["config", "user.name", "Operator"]);
    write("fn store() {}\n");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "the shared starting point"]);

    git(&["checkout", "-q", "-b", BRANCH_N2, DEFAULT_BRANCH]);
    write("fn store(keys: Keys) {}\n");
    git(&["commit", "-qam", "take the keys as a parameter"]);

    git(&["checkout", "-q", "-b", BRANCH_N3, DEFAULT_BRANCH]);
    write("fn store(tokens: Tokens) {}\n");
    git(&["commit", "-qam", "take the tokens as a parameter"]);
}

#[test]
fn a_move_whose_rebase_conflicts_records_the_git_failure_on_the_node_and_leaves_its_pr_alone() {
    // Given — n3 owns a real branch that cannot be rebased onto n2's, and an open PR #42
    let repo = tempfile::tempdir().unwrap();
    a_repo_whose_branches_conflict(repo.path());
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n2", BRANCH_N2, PR_OF_N2, &[]),
            an_open_node("n3", BRANCH_N3, PR_OF_N3, &[]),
        ],
    );
    let gh = a_stack_github();

    // When — the operator moves n3 under n2
    let result = set_stack_node_parents(
        dir,
        repo.path(),
        "n3",
        &["n2".to_string()],
        DEFAULT_BRANCH,
        &gh,
    );

    // Then — the operator is told which operation failed and on which branch
    assert_rejected(result)
        .with_reason_containing("set_stack_node_parents")
        .with_reason_containing(&format!("rebase of {BRANCH_N3} onto {BRANCH_N2} failed"));

    // and the node carries git's own message, so the conflict is diagnosable from the plan alone.
    // Matched on the command rather than compared whole: the message quotes git's stderr, which names
    // the fork-point sha the rebase was given.
    let status = stack_of(dir).node("n3").unwrap().pr_status.clone().unwrap();
    assert_eq!(status.phase, "error");
    let error = status.error.expect("a failed rebase must record why");
    assert!(
        error.contains(&format!("git rebase --onto {BRANCH_N2}")),
        "the recorded error must carry the git failure, was '{error}'"
    );

    // and the pull request still targets the base its branch actually sits on
    assert_eq!(gh.patched_bases(), Vec::<(u64, String)>::new());

    // The plan change itself stands: the DAG is written first, and the rebase is the attempt to make
    // reality follow it. The node is now marked errored for a human to finish.
    assert_eq!(parents_of(dir, "n3"), vec!["n2".to_string()]);
}
