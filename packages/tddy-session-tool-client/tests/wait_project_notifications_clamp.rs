//! Proof: `WaitProjectNotifications` holds a call for its `timeout_ms`, so a call bound for a
//! remote daemon over LiveKit has that timeout capped at the remote block ceiling like `Await`'s
//! (`#home-inbox` 7/8). Without it, a minute-long wait sits past every transport's deadline.
//!
//! Changeset: docs/dev/1-WIP/2026-10-25-home-inbox-home-wait.md

use pretty_assertions::assert_eq;
use tddy_session_tool_client::{clamp_remote_blocking_args, MAX_REMOTE_BLOCK_MS};

#[test]
fn wait_project_notifications_timeout_is_clamped_on_livekit() {
    // Given the request of a home agent that asks to wait a minute from a cursor
    let args = serde_json::json!({ "cursor": "7", "timeout_ms": 60_000 });

    // When it is prepared for a remote daemon
    let clamped = clamp_remote_blocking_args("WaitProjectNotifications", &args);

    // Then the timeout lands on the ceiling and the cursor is untouched
    assert_eq!(
        (clamped["timeout_ms"].as_u64(), clamped["cursor"].as_str()),
        (Some(MAX_REMOTE_BLOCK_MS), Some("7")),
        "the request as sent: {clamped}"
    );
}
