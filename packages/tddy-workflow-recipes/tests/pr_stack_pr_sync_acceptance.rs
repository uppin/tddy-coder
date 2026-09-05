//! Acceptance: pushing a node's edited title and body to its pull request.
//!
//! Editing a node is a plan operation and stays local; publishing that edit to GitHub is a separate,
//! externally-visible act the caller asks for per call. `sync_node_to_github_pr` therefore takes the
//! values explicitly instead of re-reading the node — a `PATCH` that restated an unchanged body
//! would overwrite whatever had been edited on GitHub in the meantime.
//!
//! A node that records no PR is a rejection rather than a skip: the caller asked for something that
//! cannot happen, and silently succeeding would be a fallback. So is a sync that names neither a
//! title nor a body, and so is one whose title or body is blank — GitHub refuses an empty edit and
//! answers a blank title with a 422. The fake refuses both the same way, so the rejection is catchable
//! here rather than only against api.github.com.
//!
//! PRD: docs/ft/coder/pr-stacking.md § Full control over the plan.
//! Changeset: docs/dev/changesets/2026-07-30-pr-stack-full-control.md.

mod common;

use common::{a_planned_node, an_insight_github, an_open_node, assert_rejected, write_stack};
use tddy_workflow_recipes::pr_stack::sync_node_to_github_pr;

const BRANCH_N1: &str = "feature/stack/n1";
const PR_OF_N1: u64 = 42;

#[test]
fn syncing_a_node_patches_its_prs_title_and_body() {
    // Given — n1 owns an open PR #42, recorded in its pr_status url
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![an_open_node("n1", BRANCH_N1, PR_OF_N1, &[])]);
    let gh = an_insight_github();

    // When — the operator publishes both edited fields
    let patched = sync_node_to_github_pr(
        dir,
        "n1",
        Some("Token store, split out"),
        Some("Extracted from the parent PR."),
        &gh,
    )
    .expect("syncing a node that owns a PR should succeed");

    // Then — the PR the node records is the one written to, with exactly those fields
    assert_eq!(patched, PR_OF_N1);
    assert_eq!(
        gh.patched_title_bodies(),
        vec![(
            PR_OF_N1,
            Some("Token store, split out".to_string()),
            Some("Extracted from the parent PR.".to_string())
        )]
    );
}

#[test]
fn syncing_only_a_title_patches_the_pull_request_the_node_records_and_leaves_its_body_alone() {
    // Given — n1's PR is #1234, so a hardcoded or index-derived number would be visible
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![an_open_node("n1", BRANCH_N1, 1234, &[])]);
    let gh = an_insight_github();

    // When — only the title was edited
    sync_node_to_github_pr(dir, "n1", Some("Retitled"), None, &gh)
        .expect("syncing a title alone should succeed");

    // Then — the PR named by the url the node records is the one written to, and no body is sent, so a
    // description edited on GitHub survives
    assert_eq!(
        gh.patched_title_bodies(),
        vec![(1234, Some("Retitled".to_string()), None)]
    );
}

#[test]
fn syncing_a_node_that_records_no_pr_is_rejected() {
    // Given — n1 was never started, so it has no PR to publish to
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![a_planned_node("n1", &[])]);
    let gh = an_insight_github();

    // When
    let result = sync_node_to_github_pr(dir, "n1", Some("Retitled"), None, &gh);

    // Then — refused, and nothing was sent
    assert_rejected(result).with_reason_containing("n1");
    assert_eq!(
        gh.patched_title_bodies(),
        Vec::<(u64, Option<String>, Option<String>)>::new()
    );
}

#[test]
fn syncing_an_unknown_node_is_rejected() {
    // Given — a stack that has no node called n9
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![an_open_node("n1", BRANCH_N1, PR_OF_N1, &[])]);
    let gh = an_insight_github();

    // When
    let result = sync_node_to_github_pr(dir, "n9", Some("Retitled"), None, &gh);

    // Then — refused by name; a missing node must never be read as "nothing to publish"
    assert_rejected(result).with_reason_containing("n9");
    assert_eq!(
        gh.patched_title_bodies(),
        Vec::<(u64, Option<String>, Option<String>)>::new()
    );
}

#[test]
fn syncing_a_node_without_naming_a_title_or_a_body_is_rejected() {
    // Given — n1 owns an open PR #42, so the only thing wrong with the call is that it names no field
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![an_open_node("n1", BRANCH_N1, PR_OF_N1, &[])]);
    let gh = an_insight_github();

    // When
    let result = sync_node_to_github_pr(dir, "n1", None, None, &gh);

    // Then — GitHub refuses an empty edit, so the call is reported as the failure it is
    assert_rejected(result).with_reason_containing("neither a title nor a body");
    assert_eq!(
        gh.patched_title_bodies(),
        Vec::<(u64, Option<String>, Option<String>)>::new()
    );
}

#[test]
fn syncing_a_blank_title_is_rejected_instead_of_sent_for_github_to_refuse() {
    // Given — n1 owns an open PR #42, so the only thing wrong with the call is the title it names
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![an_open_node("n1", BRANCH_N1, PR_OF_N1, &[])]);
    let gh = an_insight_github();

    // When — the title the operator asked to publish is whitespace
    let result = sync_node_to_github_pr(dir, "n1", Some("   "), None, &gh);

    // Then — a pull request title cannot be empty, so this is refused here by name rather than
    // answered with a 422 from api.github.com
    assert_rejected(result).with_reason_containing("the title given for PR #42 is blank");
    assert_eq!(
        gh.patched_title_bodies(),
        Vec::<(u64, Option<String>, Option<String>)>::new()
    );
}

#[test]
fn syncing_a_blank_body_clears_the_pull_requests_description() {
    // Given — n1 owns an open PR whose description has gone stale
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![an_open_node("n1", BRANCH_N1, PR_OF_N1, &[])]);
    let gh = an_insight_github();

    // When — the operator publishes an empty description
    sync_node_to_github_pr(dir, "n1", None, Some(""), &gh)
        .expect("clearing a description should succeed");

    // Then — unlike a title, GitHub's body is nullable and accepts an empty string, so clearing a
    // description is a legitimate edit and this surface must not be the one that cannot make it
    assert_eq!(
        gh.patched_title_bodies(),
        vec![(PR_OF_N1, None, Some(String::new()))]
    );
}
