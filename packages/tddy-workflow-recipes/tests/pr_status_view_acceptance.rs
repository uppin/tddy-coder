//! Acceptance: deriving the PR state shown in the PR-Stack view from GitHub's PR fields.
//!
//! `GetPrStatus` reports a branch's PR as open / merged / closed / draft. The mapping from
//! GitHub's `state` + `merged_at` + `draft` fields to `PrState` is pure and lives in
//! `pr_state_from_github`, so it is testable without a network round-trip.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § capability 3.

use rstest::rstest;
use tddy_workflow_recipes::orchestrate_pr_stack::github::{pr_state_from_github, PrState};

#[rstest]
#[case::merged("closed", Some("2026-07-01T00:00:00Z"), false, PrState::Merged)]
#[case::closed_unmerged("closed", None, false, PrState::Closed)]
#[case::open("open", None, false, PrState::Open)]
#[case::draft("open", None, true, PrState::Draft)]
fn derives_pr_state_from_github_fields(
    #[case] state: &str,
    #[case] merged_at: Option<&str>,
    #[case] draft: bool,
    #[case] expected: PrState,
) {
    // When
    let derived = pr_state_from_github(state, merged_at, draft);

    // Then
    assert_eq!(derived, expected);
}
