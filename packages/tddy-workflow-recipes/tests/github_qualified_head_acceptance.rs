//! Acceptance: the GitHub `head` filter is qualified as `owner:branch`.
//!
//! `GET /repos/{owner}/{repo}/pulls?head=…` requires `head` in `owner:branch` form. GitHub **ignores**
//! an unqualified value rather than rejecting it, so `?head=feature/x` returns the repository's entire
//! PR list and `arr.first()` yields an arbitrary, unrelated PR. Verified against the live API on
//! `uppin/tddy-coder`: `head=feature/session-attach-docs/attach-proto` returned 30 PRs, while
//! `head=uppin:feature/session-attach-docs/attach-proto` returned the 1 correct PR.
//!
//! This is latent in `get_pr_by_head` (a wrong PR number on screen) but live in `get_open_pr`, which
//! the orchestrator uses to re-target and merge — so an authenticated deployment could repoint or
//! merge the wrong pull request.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md (C3, D9).

use tddy_workflow_recipes::orchestrate_pr_stack::github::qualified_head;

#[test]
fn qualifies_a_bare_branch_with_the_repository_owner() {
    // Given / When
    let head = qualified_head(
        "uppin/tddy-coder",
        "feature/session-attach-docs/attach-proto",
    );

    // Then — without the owner prefix GitHub ignores the filter and returns every PR
    assert_eq!(head, "uppin:feature/session-attach-docs/attach-proto");
}

#[test]
fn leaves_an_already_qualified_head_unchanged() {
    // Given — a caller that already passed owner:branch
    // When
    let head = qualified_head("uppin/tddy-coder", "uppin:feature/x");

    // Then — qualifying twice would produce `uppin:uppin:feature/x`, which matches nothing
    assert_eq!(head, "uppin:feature/x");
}

#[test]
fn qualifies_a_branch_whose_name_contains_no_slash() {
    // Given / When
    let head = qualified_head("uppin/tddy-coder", "master");

    // Then
    assert_eq!(head, "uppin:master");
}

#[test]
fn qualifies_against_the_owner_segment_only_never_the_repository_name() {
    // Given — the repo is `owner/name`; only `owner` prefixes the head
    // When
    let head = qualified_head("acme/my-repo", "feature/x");

    // Then
    assert_eq!(head, "acme:feature/x");
}

#[test]
fn returns_the_bare_branch_when_the_repository_has_no_owner_segment() {
    // Given — a malformed repo string with no `owner/` part
    // When
    let head = qualified_head("tddy-coder", "feature/x");

    // Then — inventing an owner would silently query the wrong repository; pass the branch through
    assert_eq!(head, "feature/x");
}
