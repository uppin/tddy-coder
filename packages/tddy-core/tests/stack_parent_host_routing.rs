//! Unit: **which host** resolves a spawn's PR-stack parent.
//!
//! `stack_parent` is a bare session id, and every resolver behind it
//! ([`tddy_core::resolve_chain_base_ref`] and friends) reads
//! `unified_session_dir_path(sessions_base, parent_session_id)` on the **local** filesystem. A
//! stack whose orchestrator lives on another daemon is therefore not "not yet planned" but simply
//! absent, and the spawn is refused with
//! `could not resolve stack parent branch: session file missing: parent session not found under
//! sessions tree: …` — a message about a missing file for a session that exists perfectly well, one
//! host over.
//!
//! The spawning host cannot read the owner's sessions tree, so it has to *ask* the owner. This is
//! the pure half of that decision: given the id of the host that owns the parent, decide whether
//! this host answers from its own disk or hands the question to a peer. It is deliberately a
//! function over three strings — the daemon's routing helpers all need a live
//! `EligibleDaemonSource`, and the rule about which host owns a parent should be assertable without
//! one.
//!
//! Feature: `docs/ft/coder/pr-stacking.md`

use tddy_core::session_chain::{classify_stack_parent_route, StackParentRoute};

/// The daemon the spawn request landed on — the one creating the child session.
const SPAWNING_HOST: &str = "laptop-a";
/// The daemon whose sessions tree holds the pr-stack orchestrator.
const ORCHESTRATOR_HOST: &str = "workstation-b";
/// The orchestrator from the reported failure.
const ORCHESTRATOR_SESSION: &str = "01a04d4b-84f8-7fc0-b020-19ae73981175";

#[test]
fn a_spawn_with_no_stack_parent_has_nothing_to_resolve() {
    // Given a plain session spawn — no orchestrator behind it
    // When
    let route = classify_stack_parent_route(SPAWNING_HOST, None, ORCHESTRATOR_HOST);

    // Then the owner id is irrelevant: there is no parent to ask anyone about
    assert_eq!(route, StackParentRoute::NoParent);
}

#[test]
fn a_stack_parent_no_host_was_named_for_is_resolved_on_this_one() {
    // Given a stack parent sent without an owning host — every caller that predates the field, and
    // every single-host deployment
    // When
    let route = classify_stack_parent_route(SPAWNING_HOST, Some(ORCHESTRATOR_SESSION), "");

    // Then this host reads its own sessions tree, exactly as it does today
    assert_eq!(route, StackParentRoute::Local);
}

#[test]
fn a_stack_parent_owned_by_this_host_is_resolved_from_its_own_sessions_tree() {
    // Given an orchestrator on the same daemon the child is being spawned on
    // When
    let route =
        classify_stack_parent_route(SPAWNING_HOST, Some(ORCHESTRATOR_SESSION), SPAWNING_HOST);

    // Then naming this host is the same request as naming none — it must not forward to itself
    assert_eq!(route, StackParentRoute::Local);
}

#[test]
fn a_stack_parent_owned_by_another_host_is_resolved_on_that_host() {
    // Given the reported case: the child is spawned on `laptop-a`, the orchestrator lives on
    // `workstation-b`
    // When
    let route =
        classify_stack_parent_route(SPAWNING_HOST, Some(ORCHESTRATOR_SESSION), ORCHESTRATOR_HOST);

    // Then the question goes to the host that can actually answer it
    assert_eq!(
        route,
        StackParentRoute::OwnedByPeer {
            daemon_instance_id: ORCHESTRATOR_HOST.to_string()
        }
    );
}

#[test]
fn a_blank_owning_host_reads_as_no_host_named() {
    // Given a form that submits whitespace where it holds no selection
    // When
    let route = classify_stack_parent_route(SPAWNING_HOST, Some(ORCHESTRATOR_SESSION), "   ");

    // Then it is unaddressed, not a host named `"   "` that no common room can route to
    assert_eq!(route, StackParentRoute::Local);
}

#[test]
fn a_blank_stack_parent_is_no_stack_parent() {
    // Given a spawn whose `stack_parent` field is present but empty — the wire default for "none"
    // When
    let route = classify_stack_parent_route(SPAWNING_HOST, Some("   "), ORCHESTRATOR_HOST);

    // Then nothing is asked of the named host: an empty id names no session it could resolve
    assert_eq!(route, StackParentRoute::NoParent);
}
