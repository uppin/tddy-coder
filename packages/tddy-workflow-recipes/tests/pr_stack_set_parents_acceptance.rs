//! Acceptance: moving a node to a new place in the stack DAG.
//!
//! `set_stack_node_parents` is the plan-level move, distinct from `repoint_planned_pr_node`.
//! Repointing answers "the base branch drifted — retain the parent that owns this target"; this
//! answers "the plan changed — this node belongs *here* now", with the caller naming the complete
//! new parent set. Since a stack's order is derived entirely from `parents`, this is also the only
//! reorder primitive there is.
//!
//! Validation is not politeness. `Stack::topo_order` counts in-degree only over parents that
//! resolve to a node, so an unknown parent id would be silently ignored by every existing check and
//! the stack would quietly describe an edge that does not exist. A rejected call must therefore
//! leave the stack on disk byte-for-byte unchanged, which is what each rejection test asserts.
//!
//! `repo_root` is a bare tempdir here, so `local_branch_exists` is false and the git rebase is
//! deterministically skipped — the assertions are about `Changeset.stack` and about which GitHub
//! calls were made.
//!
//! PRD: docs/ft/coder/pr-stacking.md § Full control over the plan.
//! Changeset: docs/dev/changesets.md (2026-07-30, pr-stack-full-control).

mod common;

use common::{
    a_planned_node, a_stack_github, an_open_node, assert_rejected, parents_of, write_stack,
    A_STACK_PR, DEFAULT_BRANCH,
};
use tddy_workflow_recipes::pr_stack::set_stack_node_parents;

const BRANCH_N1: &str = "feature/stack/n1";
const BRANCH_N2: &str = "feature/stack/n2";
const BRANCH_N3: &str = "feature/stack/n3";
/// The PR number `FakeStackGithub` reports for whichever branch it is asked about.
const PR_OF_N3: u64 = A_STACK_PR;

fn parents(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

/// The two assertions every rejection test makes about GitHub: a call refused by validation must not
/// have looked a pull request up, and must certainly not have re-targeted one — a validation that
/// rejected *after* re-targeting would leave the PR describing a plan that was never written.
fn assert_github_untouched(gh: &common::FakeStackGithub) {
    assert_eq!(gh.looked_up(), Vec::<String>::new());
    assert_eq!(gh.patched_bases(), Vec::<(u64, String)>::new());
}

// --- tests ------------------------------------------------------------------

#[test]
fn setting_parents_on_a_plan_only_node_rewrites_the_dag_without_calling_github() {
    // Given — n3 was never started, so it owns no branch and has no PR of its own
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", BRANCH_N1, 1, &[]),
            an_open_node("n2", BRANCH_N2, 2, &[]),
            a_planned_node("n3", &["n1"]),
        ],
    );
    let gh = a_stack_github();

    // When — the operator moves it under n2 instead
    set_stack_node_parents(dir, dir, "n3", &parents(&["n2"]), DEFAULT_BRANCH, &gh)
        .expect("moving a plan-only node should succeed");

    // Then — the plan changed and nothing was asked of GitHub
    assert_eq!(parents_of(dir, "n3"), parents(&["n2"]));
    assert_eq!(gh.looked_up(), Vec::<String>::new());
    assert_eq!(gh.patched_bases(), Vec::<(u64, String)>::new());
}

#[test]
fn setting_parents_on_a_branch_owning_node_patches_its_open_prs_base_to_the_new_effective_base() {
    // Given — n3 owns a branch and an open PR, and currently stacks on n1
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", BRANCH_N1, 1, &[]),
            an_open_node("n2", BRANCH_N2, 2, &[]),
            an_open_node("n3", BRANCH_N3, PR_OF_N3, &["n1"]),
        ],
    );
    let gh = a_stack_github();

    // When — the operator moves it under n2
    set_stack_node_parents(dir, dir, "n3", &parents(&["n2"]), DEFAULT_BRANCH, &gh)
        .expect("moving a branch-owning node should succeed");

    // Then — the PR now targets n2's branch
    assert_eq!(parents_of(dir, "n3"), parents(&["n2"]));
    assert_eq!(
        gh.patched_bases(),
        vec![(PR_OF_N3, BRANCH_N2.to_string())],
        "the open PR's base must follow the node's new parent"
    );
}

#[test]
fn emptying_a_nodes_parents_makes_it_base_off_the_stack_bottom() {
    // Given — n3 stacks on n1 and owns an open PR
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", BRANCH_N1, 1, &[]),
            an_open_node("n3", BRANCH_N3, PR_OF_N3, &["n1"]),
        ],
    );
    let gh = a_stack_github();

    // When — the operator detaches it entirely
    set_stack_node_parents(dir, dir, "n3", &[], DEFAULT_BRANCH, &gh)
        .expect("detaching a node should succeed");

    // Then — it is a root, and its PR targets the stack's default branch
    assert_eq!(parents_of(dir, "n3"), Vec::<String>::new());
    assert_eq!(
        gh.patched_bases(),
        vec![(PR_OF_N3, DEFAULT_BRANCH.to_string())]
    );
}

#[test]
fn naming_an_unknown_parent_is_rejected_and_the_dag_on_disk_is_unchanged() {
    // Given — a two-node stack with no node called n9
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", BRANCH_N1, 1, &[]),
            a_planned_node("n3", &["n1"]),
        ],
    );
    let gh = a_stack_github();

    // When
    let result = set_stack_node_parents(dir, dir, "n3", &parents(&["n9"]), DEFAULT_BRANCH, &gh);

    // Then — refused by name, and n3 still stacks where it did
    assert_rejected(result).with_reason_containing("n9");
    assert_eq!(parents_of(dir, "n3"), parents(&["n1"]));
    assert_github_untouched(&gh);
}

#[test]
fn naming_the_node_itself_as_its_own_parent_is_rejected() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", BRANCH_N1, 1, &[]),
            a_planned_node("n3", &["n1"]),
        ],
    );
    let gh = a_stack_github();

    // When
    let result = set_stack_node_parents(dir, dir, "n3", &parents(&["n3"]), DEFAULT_BRANCH, &gh);

    // Then
    assert_rejected(result).with_reason_containing("n3");
    assert_eq!(parents_of(dir, "n3"), parents(&["n1"]));
    assert_github_untouched(&gh);
}

#[test]
fn a_parent_change_that_would_close_a_cycle_is_rejected_and_the_dag_on_disk_is_unchanged() {
    // Given — a straight chain n1 → n2 → n3
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            a_planned_node("n1", &[]),
            a_planned_node("n2", &["n1"]),
            a_planned_node("n3", &["n2"]),
        ],
    );
    let gh = a_stack_github();

    // When — the operator tries to stack the chain's root on its own descendant
    let result = set_stack_node_parents(dir, dir, "n1", &parents(&["n3"]), DEFAULT_BRANCH, &gh);

    // Then
    assert_rejected(result).with_reason_containing("cycle");
    assert_eq!(parents_of(dir, "n1"), Vec::<String>::new());
    assert_github_untouched(&gh);
}

#[test]
fn a_repeated_parent_id_is_rejected() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", BRANCH_N1, 1, &[]),
            a_planned_node("n3", &["n1"]),
        ],
    );
    let gh = a_stack_github();

    // When
    let result =
        set_stack_node_parents(dir, dir, "n3", &parents(&["n1", "n1"]), DEFAULT_BRANCH, &gh);

    // Then — a repeated edge is a caller mistake, not a two-parent diamond
    assert_rejected(result).with_reason_containing("n1");
    assert_eq!(parents_of(dir, "n3"), parents(&["n1"]));
    assert_github_untouched(&gh);
}
