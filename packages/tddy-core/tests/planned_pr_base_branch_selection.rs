//! Unit tests for the spawn-seam base-ref precedence: an explicit operator-chosen
//! `selected_integration_base_ref` (sent from the web Start-session dialog's "Base branch"
//! selector) wins over the stack-parent-resolved chain base; an empty override falls through
//! to the stack-parent resolution (today's behavior).
//!
//! These tests define the API contract for `tddy_core::session_chain::select_worktree_base_ref`,
//! a pure helper the daemon spawn paths call to choose the value handed to
//! `setup_worktree_for_session_with_optional_chain_base`. They fail (red) until the helper is
//! added.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-07-27-planned-pr-base-branch-selection.md
//! Changeset: docs/dev/1-WIP/2026-07-27-planned-pr-base-branch-selection.md

use tddy_core::session_chain::select_worktree_base_ref;

/// **returns_the_explicit_override_when_it_is_non_empty** — A non-empty operator-chosen base
/// ref wins over the stack-parent-resolved chain base, so a planned-PR child worktree bases off
/// the branch the operator picked in the dialog rather than the first parent the resolver walks.
#[test]
fn select_worktree_base_ref_returns_the_explicit_override_when_it_is_non_empty() {
    // Given
    let explicit = "feature/session-attach-docs/attach-store";
    let chain_base = Some("origin/feature/session-attach-docs/attach-proto".to_string());

    // When
    let result = select_worktree_base_ref(explicit, chain_base);

    // Then
    assert_eq!(
        result,
        Some("feature/session-attach-docs/attach-store".to_string()),
        "the explicit operator-chosen base must win over the stack-parent-resolved chain base"
    );
}

/// **trims_whitespace_before_deciding** — A base ref that is only whitespace is treated as
/// absent, so an accidental blank does not become the worktree base.
#[test]
fn select_worktree_base_ref_trims_whitespace_before_deciding() {
    // Given
    let explicit = "   ";
    let chain_base = Some("origin/feature/stack/parent".to_string());

    // When
    let result = select_worktree_base_ref(explicit, chain_base);

    // Then
    assert_eq!(
        result,
        Some("origin/feature/stack/parent".to_string()),
        "a whitespace-only override must fall through to the stack-parent resolution"
    );
}

/// **falls_through_to_the_chain_base_when_the_override_is_empty** — An empty override preserves
/// today's behavior: the worktree bases off the stack-parent-resolved chain base.
#[test]
fn select_worktree_base_ref_falls_through_to_the_chain_base_when_the_override_is_empty() {
    // Given
    let explicit = "";
    let chain_base = Some("origin/feature/stack/parent".to_string());

    // When
    let result = select_worktree_base_ref(explicit, chain_base);

    // Then
    assert_eq!(
        result,
        Some("origin/feature/stack/parent".to_string()),
        "an empty override must fall through to the stack-parent resolution"
    );
}

/// **returns_none_when_the_override_is_empty_and_the_chain_base_is_none** — A standalone session
/// (no stack parent, no override) bases off the default base, expressed as `None` so worktree setup
/// resolves the default live.
#[test]
fn select_worktree_base_ref_returns_none_when_the_override_is_empty_and_the_chain_base_is_none() {
    // Given
    let explicit = "";
    let chain_base: Option<String> = None;

    // When
    let result = select_worktree_base_ref(explicit, chain_base);

    // Then
    assert_eq!(
        result, None,
        "no override and no chain base must yield None (default base)"
    );
}

/// **returns_the_explicit_override_even_when_the_chain_base_is_none** — The operator's choice is
/// honored even when there is no stack-parent resolution (defensive: the web sends an override
/// only under a stack parent today, but the helper must not depend on that invariant).
#[test]
fn select_worktree_base_ref_returns_the_explicit_override_even_when_the_chain_base_is_none() {
    // Given
    let explicit = "feature/stack/chosen";
    let chain_base: Option<String> = None;

    // When
    let result = select_worktree_base_ref(explicit, chain_base);

    // Then
    assert_eq!(
        result,
        Some("feature/stack/chosen".to_string()),
        "the explicit override must win even when no chain base was resolved"
    );
}

/// **trims_a_non_empty_override_before_returning_it** — The returned base ref is the trimmed
/// value, so surrounding whitespace does not leak into the git worktree command.
#[test]
fn select_worktree_base_ref_trims_a_non_empty_override_before_returning_it() {
    // Given
    let explicit = "  feature/stack/chosen  ";
    let chain_base = Some("origin/feature/stack/parent".to_string());

    // When
    let result = select_worktree_base_ref(explicit, chain_base);

    // Then
    assert_eq!(
        result,
        Some("feature/stack/chosen".to_string()),
        "the returned base ref must be trimmed"
    );
}
