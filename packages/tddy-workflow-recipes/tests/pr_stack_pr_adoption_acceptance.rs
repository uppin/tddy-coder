//! Acceptance: bringing an existing pull request into the stack.
//!
//! A PR created outside the orchestrator — by hand, by another session, by a teammate — could never
//! join the stack. The only whole-plan rewrite path (`reseed_stack_from_plan_if_unspawned`) refuses
//! once any node owns a branch or a session, so the moment a stack became real its membership was
//! fixed. Adoption is the way in.
//!
//! An adopted node owns a `branch` and a `pr_status` from the start but no `session_id`: the PR has a
//! branch and a pull request, and no child session in *this* orchestrator. `internal_status` is left
//! unset for `pr_stack_status` to derive — pinned against the persisted node by the unit tests beside
//! `adopt_pr_as_stack_node`, which is all `adopt_pr_into_stack` delegates that part to.
//!
//! PRD: docs/ft/coder/pr-stacking.md § Full control over the plan.
//! Changeset: docs/dev/changesets/2026-07-30-pr-stack-full-control.md.

mod common;

use common::{
    a_planned_node, a_pr, an_insight_github, an_open_node, assert_rejected, node_ids, stack_of,
    write_stack, REPO,
};
use rstest::rstest;
use tddy_workflow_recipes::orchestrate_pr_stack::github::PrState;
use tddy_workflow_recipes::pr_stack::adopt_pr_into_stack;

const ADOPTED_BRANCH: &str = "feature/written-elsewhere";
const BASE: &str = "master";
const PR: u64 = 77;

#[test]
fn adopting_a_pr_reads_its_title_body_and_head_branch_from_github() {
    // Given — an empty plan and a PR that exists on GitHub
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![]);
    let gh = an_insight_github().with_pr(a_pr(PR, ADOPTED_BRANCH, BASE));

    // When
    let node =
        adopt_pr_into_stack(dir, PR, vec![], &gh).expect("adopting an existing PR should succeed");

    // Then — the node describes the PR, and records where to find it
    assert_eq!(node.node_id, "n1");
    assert_eq!(node.title, format!("PR {PR}"));
    assert_eq!(node.description, format!("body of PR {PR}"));
    assert_eq!(node.branch.as_deref(), Some(ADOPTED_BRANCH));
    assert_eq!(
        node.pr_status.as_ref().unwrap().url.as_deref(),
        Some(format!("https://github.com/{REPO}/pull/{PR}").as_str())
    );
}

#[rstest]
#[case::open(PrState::Open, "open")]
#[case::draft(PrState::Draft, "open")]
#[case::merged(PrState::Merged, "merged")]
#[case::closed(PrState::Closed, "closed")]
fn an_adopted_node_records_the_prs_live_state_as_its_phase(
    #[case] live_state: PrState,
    #[case] expected_phase: &str,
) {
    // Given — the same PR in each of the states GitHub can report
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![]);
    let gh = an_insight_github().with_pr(
        tddy_workflow_recipes::orchestrate_pr_stack::github::PrDetail {
            state: live_state,
            ..a_pr(PR, ADOPTED_BRANCH, BASE)
        },
    );

    // When
    let node = adopt_pr_into_stack(dir, PR, vec![], &gh).expect("adopting should succeed");

    // Then — a draft is an open PR; the phase vocabulary stays the documented one
    assert_eq!(node.pr_status.as_ref().unwrap().phase, expected_phase);
}

#[test]
fn an_adopted_pr_can_be_stacked_onto_existing_nodes() {
    // Given — a two-node plan the adopted PR should sit behind
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            an_open_node("n1", "feature/stack/n1", 1, &[]),
            a_planned_node("n2", &["n1"]),
        ],
    );
    let gh = an_insight_github().with_pr(a_pr(PR, ADOPTED_BRANCH, BASE));

    // When
    let node = adopt_pr_into_stack(dir, PR, vec!["n2".to_string()], &gh)
        .expect("adopting onto an existing node should succeed");

    // Then — appended with the next free id and the chosen ancestor
    assert_eq!(node.node_id, "n3");
    assert_eq!(node.parents, vec!["n2".to_string()]);
    assert_eq!(node_ids(dir), vec!["n1", "n2", "n3"]);
}

#[test]
fn adopting_a_pr_whose_head_branch_is_already_bound_to_a_node_is_rejected() {
    // Given — n1 already owns the branch this PR is built on
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![an_open_node("n1", ADOPTED_BRANCH, PR, &[])]);
    let gh = an_insight_github().with_pr(a_pr(PR, ADOPTED_BRANCH, BASE));

    // When
    let result = adopt_pr_into_stack(dir, PR, vec![], &gh);

    // Then — refused, so a PR cannot be tracked twice, and nothing was appended
    assert_rejected(result).with_reason_containing(ADOPTED_BRANCH);
    assert_eq!(node_ids(dir), vec!["n1"]);
}

#[test]
fn adopting_a_pr_with_a_dangling_parent_ref_is_rejected_and_nothing_is_appended() {
    // Given — a plan with no node called n9
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![an_open_node("n1", "feature/stack/n1", 1, &[])]);
    let gh = an_insight_github().with_pr(a_pr(PR, ADOPTED_BRANCH, BASE));

    // When
    let result = adopt_pr_into_stack(dir, PR, vec!["n9".to_string()], &gh);

    // Then
    assert_rejected(result).with_reason_containing("n9");
    assert_eq!(node_ids(dir), vec!["n1"]);
    assert_eq!(stack_of(dir).nodes.len(), 1);
}
