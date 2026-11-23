//! Unit tests: when a repoint needs the project's GitHub token, and what a refusal to resolve one
//! does to the call.
//!
//! A repoint of a node that owns no branch is plan-only — nothing rebased, pushed or re-targeted —
//! so it asks GitHub for nothing and must not be refused for want of an account. One that owns a
//! branch re-targets its PR, so it needs the token and is refused, before any plan is rewritten,
//! when there is none.
//!
//! Changeset: docs/dev/1-WIP/2026-09-19-keyring-github-identity.md

use super::guards::token_for_repoint;
use tddy_rpc::Code;

#[test]
fn a_plan_only_repoint_asks_for_no_token() {
    // Given a node that owns no branch
    let mut asked = false;

    // When the repoint's token is resolved
    let token = token_for_repoint(false, || {
        asked = true;
        Err("no account assigned".to_string())
    })
    .expect("a plan-only repoint is never refused");

    // Then nothing was asked of the vault, and the repoint proceeds without a token
    assert_eq!(token, None);
    assert!(!asked);
}

#[test]
fn a_branch_owning_repoint_uses_the_token_the_project_resolves() {
    // Given a node that owns a branch, in a project whose account resolves
    // When the repoint's token is resolved
    let token = token_for_repoint(true, || Ok("ghp_project_account".to_string()))
        .expect("an account resolves");

    // Then the repoint carries that token
    assert_eq!(token, Some("ghp_project_account".to_string()));
}

#[test]
fn a_branch_owning_repoint_is_refused_with_the_reason_when_the_project_resolves_no_account() {
    // Given a node that owns a branch, in a project that assigns no account
    let reason = "this project has no github account assigned; assign one to the project";

    // When the repoint's token is resolved
    let refusal = token_for_repoint(true, || Err(reason.to_string()))
        .expect_err("a repoint that re-targets a PR needs an account");

    // Then it is a precondition failure carrying the resolver's own words, not an internal error
    assert_eq!(refusal.code, Code::FailedPrecondition);
    assert_eq!(refusal.message(), reason);
}
