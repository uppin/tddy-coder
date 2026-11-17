//! Proof: `WaitSessionWatchEvents` holds a call for its `timeout_ms`, so a monitor's call bound for
//! a remote daemon over LiveKit has that timeout capped at the remote block ceiling, as
//! `WaitProjectNotifications`' is (`#activity-wake` 4/5).
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-watch-monitor.md

use pretty_assertions::assert_eq;
use tddy_session_tool_client::{clamp_remote_blocking_args, MAX_REMOTE_BLOCK_MS};

#[test]
fn the_stream_wait_is_clamped_for_a_remote_call() {
    // Given a monitor's call that asks to wait a minute, acknowledging what it printed
    let args = serde_json::json!({
        "watch_ids": ["w-1"],
        "stream_id": "s-1",
        "ack_seq": 41,
        "timeout_ms": 60_000
    });

    // When it is prepared for a remote daemon
    let clamped = clamp_remote_blocking_args("WaitSessionWatchEvents", &args);

    // Then the timeout lands on the ceiling and the rest of the call is untouched
    assert_eq!(
        (
            clamped["timeout_ms"].as_u64(),
            clamped["ack_seq"].as_u64(),
            clamped["stream_id"].as_str(),
        ),
        (Some(MAX_REMOTE_BLOCK_MS), Some(41), Some("s-1")),
        "the request as sent: {clamped}"
    );
}
