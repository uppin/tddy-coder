//! Unit tests: which session types own a worktree that `DeleteSession` should remove.
//!
//! `session_deletion.rs` decides this inline, filtering on `session_type == "claude-cli"`. Extracting
//! the predicate makes the boundary statable — and the boundary matters, because it is the
//! difference between removing a session's own worktree and removing the project checkout every
//! other session shares.
//!
//! Kept separate from `workspace_session_deletion_acceptance.rs` deliberately: that suite compiles
//! and runs against today's code, demonstrating the leak as a real failure. Importing a
//! not-yet-existing symbol into it would turn that runnable proof into a compile error and lose the
//! evidence.

use tddy_daemon::session_deletion::worktree_removal_applies_to;

#[test]
fn a_workspace_session_is_eligible_for_worktree_removal() {
    // Then — a workspace session *is* a worktree and nothing else; leaving it behind leaks the only
    // thing the session was for
    assert!(worktree_removal_applies_to("workspace"));
}

#[test]
fn a_claude_cli_session_remains_eligible_for_worktree_removal() {
    // Then — the pre-existing behaviour, which widening the predicate must not disturb
    assert!(worktree_removal_applies_to("claude-cli"));
}

#[test]
fn a_tool_session_is_not_eligible_for_worktree_removal() {
    // Then — a tddy-coder session records the project's *main repo* rather than a worktree of its
    // own, so removing it would delete the shared checkout
    assert!(!worktree_removal_applies_to("tool"));
}

#[test]
fn a_session_with_no_recorded_type_is_not_eligible_for_worktree_removal() {
    // Then — legacy `.session.yaml` files predate `session_type`. Treating an unknown type as
    // removable would make deletion destructive on exactly the files we know least about.
    assert!(!worktree_removal_applies_to(""));
}

#[test]
fn a_cursor_cli_session_is_deliberately_left_out_of_scope() {
    // Then — cursor-cli leaks its worktree the same way `workspace` does, but fixing it here would
    // change behaviour for sessions split placement never touches. Tracked in docs/dev/TODO.md.
    // Pinned so the omission stays a visible decision rather than an oversight rediscovered later.
    assert!(!worktree_removal_applies_to("cursor-cli"));
}
