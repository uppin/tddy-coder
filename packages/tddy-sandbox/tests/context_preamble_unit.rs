//! Unit tests: rendering the managed-codebase preamble, prepending it, and the staleness line.
//!
//! The preamble is the only thing standing between an agent and a directory that looks like a
//! repository but is not one. It has to say so before the project's own instructions, survive being
//! composed with a file of any shape, and be removable again without disturbing what it was
//! prepended to.
//!
//! The composition functions here are the pure core the filesystem-level helpers wrap; the
//! filesystem behaviour itself is `context_dir_sync_acceptance.rs` and `context_manifest_acceptance.rs`.
//!
//! PRD: docs/ft/daemon/agent-context-sync.md § Design.

use pretty_assertions::assert_eq;
use tddy_sandbox::{
    managed_codebase_preamble, prepend_preamble, with_stale_marker, without_stale_marker,
    SubagentReplacement, MANAGED_CODEBASE_PREAMBLE,
};

// ---------------------------------------------------------------------------
// What the preamble says
// ---------------------------------------------------------------------------

/// With no subagents the rendered preamble is the constant, byte for byte — the same relationship
/// the appendix had to `SANDBOX_REMOTE_APPENDIX`, so nothing about the plain case changed when it
/// moved to the front.
#[test]
fn with_no_subagents_the_rendered_preamble_is_the_constant_itself() {
    // Then
    assert_eq!(managed_codebase_preamble(&[]), MANAGED_CODEBASE_PREAMBLE);
}

/// The rule the whole managed-codebase arrangement rests on: native tools do not reach the
/// codebase, the `mcp__tddy-tools__*` ones do.
#[test]
fn the_preamble_directs_every_file_and_shell_operation_through_the_tddy_tools() {
    // When
    let preamble = managed_codebase_preamble(&[]);

    // Then
    assert!(preamble.contains("mcp__tddy-tools__"));
    for tool in ["Read", "Write", "Grep", "Glob", "Shell"] {
        assert!(
            preamble.contains(tool),
            "the preamble must list {tool} among the tools to use: {preamble}"
        );
    }
}

/// A withdrawn tool has somewhere to go, and the preamble says where — otherwise the agent
/// discovers mid-turn that a tool it can see is refused, with nothing naming the agent to ask.
#[test]
fn a_withdrawn_tool_is_named_beside_the_subagent_that_took_it() {
    // When
    let preamble = managed_codebase_preamble(&[SubagentReplacement {
        name: "explorer",
        replaced: &["Grep", "Glob"],
    }]);

    // Then
    assert!(preamble.contains("explorer"));
    assert!(preamble.contains("Grep"));
    assert!(preamble.contains("Glob"));
}

/// A replaced `Shell` gets the session-actions paragraph rather than the generic delegation hint —
/// commands then run only through `request_action` / `invoke_action`.
#[test]
fn a_replaced_shell_gets_the_session_actions_paragraph() {
    // When
    let preamble = managed_codebase_preamble(&[SubagentReplacement {
        name: "commander",
        replaced: &["Shell"],
    }]);

    // Then
    assert!(preamble.contains("request_action"));
    assert!(preamble.contains("invoke_action"));
}

// ---------------------------------------------------------------------------
// Prepending
// ---------------------------------------------------------------------------

/// The preamble leads and the body follows, whole. This is the change from appending: the rule is
/// read before however many thousand words the project's own file holds.
#[test]
fn prepending_puts_the_preamble_first_and_keeps_the_body_intact() {
    // Given
    let body = "# Project rules\n\nAlways run ./test.\n";

    // When
    let composed = prepend_preamble(MANAGED_CODEBASE_PREAMBLE, body);

    // Then
    assert!(composed.starts_with(MANAGED_CODEBASE_PREAMBLE.trim_start()));
    assert!(composed.ends_with(body));
}

/// An empty body yields the preamble alone — the case of a target repo with no `CLAUDE.md`, where
/// the agent still has to be told where its codebase is.
#[test]
fn prepending_onto_an_empty_body_yields_the_preamble_alone() {
    // When
    let composed = prepend_preamble(MANAGED_CODEBASE_PREAMBLE, "");

    // Then
    assert_eq!(composed.trim(), MANAGED_CODEBASE_PREAMBLE.trim());
}

/// Composing twice does not stack two preambles. A re-sync rewrites `CLAUDE.md` from the repo's
/// bytes every time it changes, and a bug that fed it an already-composed file would otherwise
/// double the notice on every tick.
#[test]
fn prepending_onto_an_already_composed_file_does_not_stack_a_second_preamble() {
    // Given
    let once = prepend_preamble(MANAGED_CODEBASE_PREAMBLE, "# Project rules\n");

    // When
    let twice = prepend_preamble(MANAGED_CODEBASE_PREAMBLE, &once);

    // Then
    assert_eq!(twice, once);
}

/// The two are separated, so the preamble's last line and the project's first heading do not run
/// together into one markdown block.
#[test]
fn the_preamble_and_the_body_are_separated_by_a_blank_line() {
    // Given
    let body = "# Project rules\n";

    // When
    let composed = prepend_preamble(MANAGED_CODEBASE_PREAMBLE, body);

    // Then
    let boundary = composed
        .find("# Project rules")
        .expect("body must be present");
    assert!(
        composed[..boundary].ends_with("\n\n"),
        "the body must start after a blank line; got {:?}",
        &composed[boundary.saturating_sub(20)..boundary]
    );
}

// ---------------------------------------------------------------------------
// The staleness line
// ---------------------------------------------------------------------------

/// A failed re-sync says so where the agent reads: inside the preamble, above the guidance it can
/// no longer vouch for.
#[test]
fn the_stale_marker_lands_inside_the_preamble_not_at_the_end_of_the_file() {
    // Given
    let composed = prepend_preamble(MANAGED_CODEBASE_PREAMBLE, "# Project rules\n");

    // When
    let stale = with_stale_marker(&composed);

    // Then
    let marker = stale.find("STALE").expect("the marker must be present");
    let body = stale
        .find("# Project rules")
        .expect("the body must survive");
    assert!(
        marker < body,
        "the staleness line must sit above the guidance it qualifies"
    );
}

/// Removing it restores the file exactly, so a recovered session's `CLAUDE.md` is byte-identical to
/// one that never failed.
#[test]
fn removing_the_stale_marker_restores_the_original_bytes() {
    // Given
    let composed = prepend_preamble(MANAGED_CODEBASE_PREAMBLE, "# Project rules\n");

    // When
    let restored = without_stale_marker(&with_stale_marker(&composed));

    // Then
    assert_eq!(restored, composed);
}

/// Marking an already-marked file is a no-op. A link down for ten ticks must not bury the guidance
/// under ten identical warnings.
#[test]
fn marking_an_already_stale_file_changes_nothing() {
    // Given
    let once = with_stale_marker(&prepend_preamble(MANAGED_CODEBASE_PREAMBLE, "# rules\n"));

    // When
    let twice = with_stale_marker(&once);

    // Then
    assert_eq!(twice, once);
}

/// Clearing a file that was never marked is a no-op too, so the syncer can call it unconditionally
/// on every success without reading first.
#[test]
fn clearing_a_file_that_was_never_stale_changes_nothing() {
    // Given
    let composed = prepend_preamble(MANAGED_CODEBASE_PREAMBLE, "# rules\n");

    // When
    let cleared = without_stale_marker(&composed);

    // Then
    assert_eq!(cleared, composed);
}

/// The marker text is distinctive enough that a project whose own `CLAUDE.md` discusses stale data
/// does not get its content mangled by the clear.
#[test]
fn a_body_that_merely_mentions_the_word_stale_is_not_disturbed_by_clearing() {
    // Given
    let body = "# Project rules\n\nRefuse to serve a STALE cache entry.\n";
    let composed = prepend_preamble(MANAGED_CODEBASE_PREAMBLE, body);

    // When
    let cleared = without_stale_marker(&with_stale_marker(&composed));

    // Then
    assert_eq!(cleared, composed);
    assert!(cleared.contains("Refuse to serve a STALE cache entry."));
}
