//! Integration tests: an `opencode-cli` session is facilitated by the daemon it was started on —
//! it runs its agent there, so `ConnectSession` and the room extensions treat it as they treat a
//! `claude-cli` or `cursor-cli` session.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-opencode-cli.md

use std::path::PathBuf;

use tddy_agent_launch::session_coordinate_handlers::{facilitated_checkout, FacilitatedCheckout};
use tddy_core::SessionMetadata;

fn a_session_of_type(session_type: &str, worktree: Option<&str>) -> SessionMetadata {
    let mut record = serde_json::json!({
        "session_id": "019fb0c1-5a2e-7d11-9c3b-6f0a1e2d3c4b",
        "project_id": "opencode-project",
        "created_at": "2026-10-20T05:03:46Z",
        "updated_at": "2026-10-20T05:03:46Z",
        "status": "active",
        "session_type": session_type,
    });
    if let Some(worktree) = worktree {
        record["repo_path"] = worktree.into();
    }
    serde_json::from_value(record).expect("a session record")
}

#[test]
fn an_opencode_session_is_facilitated_over_its_worktree() {
    // Given
    let session = a_session_of_type("opencode-cli", Some("/work/opencode-session"));

    // When / Then
    assert_eq!(
        facilitated_checkout(&session),
        FacilitatedCheckout::Checkout(PathBuf::from("/work/opencode-session"))
    );
}

#[test]
fn an_opencode_session_with_no_worktree_is_facilitated_over_no_checkout() {
    // Given
    let session = a_session_of_type("opencode-cli", None);

    // When / Then
    assert_eq!(
        facilitated_checkout(&session),
        FacilitatedCheckout::NoCheckout
    );
}

#[test]
fn a_workspace_session_is_still_not_facilitated() {
    // Given — the rule widened by one type, not into a catch-all
    let session = a_session_of_type("workspace", Some("/work/clone"));

    // When / Then
    assert_eq!(
        facilitated_checkout(&session),
        FacilitatedCheckout::NotFacilitated
    );
}
