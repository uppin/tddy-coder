//! The refusal a main-agent answer receives while the turn holding its request is still running. It names the turn
//! that holds the request, and where its outcome goes: the call that deferred the request, not the turn that
//! deferred it. A held answer may be for a request the main agent kept, so "the turn that deferred it" was loose.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use tddy_discovery::subagent::answer_delivered_while_held;

#[test]
fn the_refusal_of_a_held_answer_names_the_turn_holding_the_request() {
    // When the refusal is given for a delivered answer
    let refusal = answer_delivered_while_held("perm-1");

    // Then it names the turn holding the request, and the call the outcome goes to
    assert!(
        refusal.contains("the turn holding it carries on"),
        "{refusal}"
    );
    assert!(
        refusal.contains("its outcome goes to the call that deferred the request"),
        "{refusal}"
    );
    assert!(
        refusal.contains("permission request 'perm-1' was answered"),
        "{refusal}"
    );
}

#[test]
fn the_refusal_of_a_held_answer_no_longer_says_the_turn_that_deferred_the_request() {
    // When the refusal is given for a delivered answer
    let refusal = answer_delivered_while_held("perm-1");

    // Then the wording that said the turn that deferred it is gone
    assert!(!refusal.contains("the turn that deferred it"), "{refusal}");
}
