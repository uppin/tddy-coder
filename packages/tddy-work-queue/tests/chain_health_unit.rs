//! Leaked commits and line state per work item (`#workqueue` 12/14).

use pretty_assertions::assert_eq;
use tddy_github_inbox::PrCommit;
use tddy_work_queue::chain_health::{leaks, line_state, Leak, LineState};

fn a_commit(sha: &str, subject: &str, date: &str) -> PrCommit {
    PrCommit {
        sha: sha.to_string(),
        author_name: "dev".to_string(),
        author_date: date.to_string(),
        subject: subject.to_string(),
    }
}

fn a_leak(sha: &str, ancestor: &str) -> Leak {
    Leak {
        sha: sha.to_string(),
        ancestor: ancestor.to_string(),
    }
}

#[test]
fn a_commit_with_an_ancestors_sha_is_leaked_from_it() {
    // Given
    let parent = vec![a_commit("p1", "parent work", "2026-11-22T09:00:00Z")];
    let own = vec![
        a_commit("p1", "parent work", "2026-11-22T09:00:00Z"),
        a_commit("o1", "own work", "2026-11-22T10:00:00Z"),
    ];

    // When
    let found = leaks(&own, &[("feat/parent".to_string(), parent)]);

    // Then
    assert_eq!(found, vec![a_leak("p1", "feat/parent")]);
}

#[test]
fn a_rebased_copy_with_the_same_author_date_and_subject_is_leaked() {
    // Given — the same work under a new SHA
    let parent = vec![a_commit("p1", "parent work", "2026-11-22T09:00:00Z")];
    let own = vec![a_commit(
        "p1-rebased",
        "parent work",
        "2026-11-22T09:00:00Z",
    )];

    // When
    let found = leaks(&own, &[("feat/parent".to_string(), parent)]);

    // Then
    assert_eq!(found, vec![a_leak("p1-rebased", "feat/parent")]);
}

#[test]
fn the_nearest_ancestor_carrying_a_commit_is_named() {
    // Given — the commit is in both the parent and the grandparent
    let shared = a_commit("g1", "grand work", "2026-11-22T08:00:00Z");
    let ancestors = vec![
        ("feat/parent".to_string(), vec![shared.clone()]),
        ("feat/grand".to_string(), vec![shared.clone()]),
    ];

    // When
    let found = leaks(&[shared], &ancestors);

    // Then
    assert_eq!(found, vec![a_leak("g1", "feat/parent")]);
}

#[test]
fn an_item_containing_its_parents_tip_is_linear() {
    assert_eq!(line_state(Some(0), Some(true)), LineState::Linear);
}

#[test]
fn an_item_missing_parent_commits_is_behind_by_their_count() {
    assert_eq!(line_state(Some(3), Some(true)), LineState::Behind { by: 3 });
}

#[test]
fn behind_and_unmergeable_is_conflicting() {
    assert_eq!(
        line_state(Some(2), Some(false)),
        LineState::Conflicting { by: 2 }
    );
}

#[test]
fn a_merged_or_planned_parent_gives_unknown() {
    assert_eq!(line_state(None, Some(true)), LineState::Unknown);
}
