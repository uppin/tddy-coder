//! Acceptance: which repoint targets the daemon accepts from a client.
//!
//! `RepointPlannedPrRequest.target_base_branch` is the branch the operator's "Repoint to <target>"
//! control named, and the daemon retains exactly the parents that own it — so a target no parent owns
//! means **every** parent is dropped and the node detaches onto the default branch. That makes an
//! unvalidated target a silent plan rewrite: a stale label, a typo, or a client that has drifted from
//! the daemon's view of the repo would all read as "detach this node" rather than as an error.
//!
//! So a non-empty target must name either the resolved default branch or one of the node's parents'
//! branches. Nothing else is a meaningful thing to be based onto, and an empty target is not a target
//! at all — it selects the original drop-merged-parents rule.
//!
//! The default branch is compared with `origin/` stripped from both sides. `resolve_default_integration_base_ref`
//! returns a remote-tracking ref (`origin/master`), while a parent's `branch` and a GitHub PR base are
//! plain branch names, so the label the web renders can legitimately carry either form.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § Repointing a dead-end planned PR (D18).
//! Changeset: docs/dev/changesets.md (2026-07-26, pr-stack-repoint-dead-end).

use tddy_daemon::connection_service::validate_repoint_target;

const DEFAULT_BRANCH: &str = "origin/master";
const PARENT_BRANCH: &str = "feature/attach-docs/attach-proto";

#[test]
fn an_empty_target_selects_the_drop_merged_parents_rule() {
    // Given / When — no target named, which is what a caller that is not the web sends
    let target = validate_repoint_target("", DEFAULT_BRANCH, &[PARENT_BRANCH]);

    // Then — `None` is the original behaviour, not a rejection
    assert_eq!(target, Ok(None));
}

#[test]
fn a_whitespace_only_target_is_no_target_at_all() {
    // Given / When
    let target = validate_repoint_target("   ", DEFAULT_BRANCH, &[PARENT_BRANCH]);

    // Then — a blank field must not be mistaken for a branch literally named "   "
    assert_eq!(target, Ok(None));
}

#[test]
fn the_resolved_default_branch_is_accepted() {
    // Given / When — the reported case: no parent survives, so the node detaches onto the default
    let target = validate_repoint_target(DEFAULT_BRANCH, DEFAULT_BRANCH, &[PARENT_BRANCH]);

    // Then
    assert_eq!(target, Ok(Some("origin/master".to_string())));
}

#[test]
fn the_default_branch_is_accepted_without_its_origin_prefix() {
    // Given / When — the web may render either form; both name the same ref
    let target = validate_repoint_target("master", DEFAULT_BRANCH, &[PARENT_BRANCH]);

    // Then
    assert_eq!(target, Ok(Some("master".to_string())));
}

#[test]
fn the_default_branch_is_accepted_with_an_origin_prefix_it_does_not_itself_carry() {
    // Given / When — a project whose stored default branch is a bare name, against the remote-tracking
    // form the web may render
    let target = validate_repoint_target("origin/master", "master", &[PARENT_BRANCH]);

    // Then — the comparison strips `origin/` from both sides, so neither direction is a refusal
    assert_eq!(target, Ok(Some("origin/master".to_string())));
}

#[test]
fn a_parents_own_branch_is_accepted() {
    // Given / When — one parent is dead and another is still a usable base, so the target is that
    // surviving parent's branch rather than the default
    let target = validate_repoint_target(
        "feature/attach-docs/attach-store",
        DEFAULT_BRANCH,
        &[PARENT_BRANCH, "feature/attach-docs/attach-store"],
    );

    // Then
    assert_eq!(
        target,
        Ok(Some("feature/attach-docs/attach-store".to_string()))
    );
}

#[test]
fn a_target_that_is_neither_the_default_branch_nor_a_parents_branch_is_rejected() {
    // Given / When — a stale or mistyped label that happens to name a real branch elsewhere
    let unrelated_branch = "feature/somebody-elses-work";
    let target = validate_repoint_target(unrelated_branch, DEFAULT_BRANCH, &[PARENT_BRANCH]);

    // Then — accepting it would silently drop every parent and detach the node
    assert_eq!(
        target,
        Err(format!(
            "target_base_branch '{unrelated_branch}' names neither the default branch '{DEFAULT_BRANCH}' nor any parent's branch"
        ))
    );
}

#[test]
fn a_parents_branch_is_rejected_once_that_parent_is_no_longer_a_parent() {
    // Given / When — the node has no parents left (an earlier repoint already detached it), so the only
    // acceptable target is the default branch
    let target = validate_repoint_target(PARENT_BRANCH, DEFAULT_BRANCH, &[]);

    // Then
    assert_eq!(
        target,
        Err(format!(
            "target_base_branch '{PARENT_BRANCH}' names neither the default branch '{DEFAULT_BRANCH}' nor any parent's branch"
        ))
    );
}
