//! The tokens that admit a clone's helper to the daemon's askpass endpoint.
//!
//! A token is the whole of the helper's authority: it names the operator a prompt is raised for, so
//! one that outlived its clone, or could be guessed, would let a stranger put a passphrase prompt in
//! front of somebody.
//!
//! Feature: docs/ft/daemon/clone-ssh-passphrase.md

use std::collections::HashSet;

use tddy_host_service::askpass::CloneGrants;

#[test]
fn a_token_names_the_operator_whose_clone_it_was_issued_for() {
    let grants = CloneGrants::new();

    let grant = grants.register("ada");

    assert_eq!(grants.operator_for(&grant.token()), Some("ada".to_string()));
}

#[test]
fn a_token_stops_naming_anyone_when_its_clone_ends() {
    let grants = CloneGrants::new();
    let grant = grants.register("ada");
    let token = grant.token();

    drop(grant);

    assert_eq!(grants.operator_for(&token), None);
}

#[test]
fn one_clone_ending_leaves_another_clones_token_alone() {
    let grants = CloneGrants::new();
    let first = grants.register("ada");
    let second = grants.register("grace");

    drop(first);

    assert_eq!(
        grants.operator_for(&second.token()),
        Some("grace".to_string())
    );
}

#[test]
fn a_token_nobody_issued_names_no_one() {
    assert_eq!(CloneGrants::new().operator_for("guess"), None);
}

#[test]
fn tokens_are_long_enough_and_distinct_enough_not_to_be_guessed() {
    let grants = CloneGrants::new();

    let grants_held: Vec<_> = (0..200).map(|_| grants.register("ada")).collect();
    let tokens: HashSet<String> = grants_held.iter().map(|g| g.token()).collect();

    assert_eq!(tokens.len(), 200, "two clones were given the same token");
    assert!(
        tokens.iter().all(|t| t.len() >= 32),
        "a token of fewer than 32 characters is guessable"
    );
}
