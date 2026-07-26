//! Acceptance: how a PR lookup resolves the caller's GitHub credential.
//!
//! Three outcomes, and the distinction between them is the whole point:
//!
//! - **Empty** — stub / demo authentication (`github.stub: true`). The product is demoed and tested
//!   without real GitHub credentials, so the lookup short-circuits to "no PRs": clean, successful, and
//!   indistinguishable from a repository that genuinely has none. A demo must never surface an error.
//! - **Unavailable** — a real login whose credential could not be used (absent, blank, expired,
//!   insufficiently scoped). Reported as *unavailable* with an operator-facing reason, never as
//!   "no PR exists" — the silent `Ok(None)` on a missing token is exactly why a live open PR stayed
//!   invisible on the PR-Stack screen.
//! - **Perform** — a real login with a usable token.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md (C3, D7, D8, D12).

use tddy_daemon::github_pr_credentials::{pr_lookup_for_caller, PrLookup};

#[test]
fn stub_authentication_resolves_to_an_empty_pr_result() {
    // Given — the daemon runs with github.stub: true and holds no token for the demo login
    // When
    let plan = pr_lookup_for_caller(true, None);

    // Then — a demo shows no PRs, and nothing about it is an error
    assert_eq!(
        plan,
        PrLookup::Empty,
        "a stub login must resolve to an empty PR result, never to unavailable or an error"
    );
}

#[test]
fn stub_authentication_resolves_to_an_empty_pr_result_even_when_a_token_is_present() {
    // Given — stub mode with a leftover stored token from a previous real login
    // When
    let plan = pr_lookup_for_caller(true, Some("gho_a_real_looking_token"));

    // Then — stub mode never talks to GitHub, whatever happens to be stored
    assert_eq!(
        plan,
        PrLookup::Empty,
        "stub mode must not reach GitHub even when a token is available"
    );
}

#[test]
fn a_real_login_with_a_stored_token_performs_the_lookup_with_it() {
    // Given
    // When
    let plan = pr_lookup_for_caller(false, Some("gho_stored_for_this_login"));

    // Then
    assert_eq!(
        plan,
        PrLookup::Perform("gho_stored_for_this_login".to_string())
    );
}

#[test]
fn a_real_login_without_a_stored_token_reports_the_status_unavailable() {
    // Given — a genuine GitHub login whose access token was never stored (or has been cleared)
    // When
    let plan = pr_lookup_for_caller(false, None);

    // Then — the operator must learn the status is unknown, not that no PR exists
    let PrLookup::Unavailable(reason) = plan else {
        panic!("a real login with no token must report unavailable, got: {plan:?}");
    };
    assert!(
        !reason.trim().is_empty(),
        "unavailable must carry an operator-facing reason, got an empty string"
    );
}

#[test]
fn a_blank_stored_token_reports_the_status_unavailable() {
    // Given — a stored entry that is present but empty (a truncated or cleared write)
    // When
    let plan = pr_lookup_for_caller(false, Some("   "));

    // Then — a blank token cannot authenticate, and must not be sent as if it could
    let PrLookup::Unavailable(reason) = plan else {
        panic!("a blank token must report unavailable, got: {plan:?}");
    };
    assert!(
        !reason.trim().is_empty(),
        "unavailable must carry an operator-facing reason, got an empty string"
    );
}
