//! Acceptance: the toolcall relay between `tddy-tools` and the session-owning process (today a
//! bespoke newline-delimited-JSON protocol over `TDDY_SOCKET`) must move onto `tddy-rpc`/
//! `tddy-stdio` framing — see docs/dev/TODO.md ("Migrate the toolcall listener to
//! tddy-rpc/tddy-stdio"). The wire *payloads* (the existing `*Wire` request structs and
//! `ToolCallResponse` JSON shapes) are unchanged; only the framing/multiplexing that carries them
//! over the socket changes, exactly as the sandbox tool-IPC migration already did for
//! `tddy-tools` <-> `tddy-sandbox-runner` (see `session_tool_client::dispatch_via_sandbox_ipc`).
//!
//! One test per file: `start_toolcall_listener`'s socket path is keyed only by process id, so
//! two calls within the same test binary collide — the same reason `submit_relay_no_poll.rs` and
//! `toolcall_relay_presenter_stuck.rs` each hold a single test.

use serde_json::json;
use std::time::Duration;
use tddy_core::toolcall::start_toolcall_listener;
use tddy_tools::toolcall_client::dispatch_toolcall;

/// Bounded safety net, not the expected duration — see "Testing Async Code" in the fluent-tests
/// guidelines. A real listener + real socket round trip normally completes in well under 100ms;
/// this only guards against a genuine hang while the listener still speaks the old protocol.
const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// **submit_relays_over_the_stdio_rpc_transport_to_the_toolcall_listener**: a `submit` request
/// sent via the new stdio-RPC client is acknowledged immediately with the submitted goal — the
/// same `SubmitOk` behavior the old line-JSON protocol provides, over the new transport.
#[tokio::test]
async fn submit_relays_over_the_stdio_rpc_transport_to_the_toolcall_listener() {
    // Given a real toolcall listener
    let tddy_data_dir =
        std::env::temp_dir().join(format!("tddy-toolcall-stdio-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tddy_data_dir).unwrap();
    let (socket_path, _tool_rx) =
        start_toolcall_listener(None, None, tddy_data_dir).expect("start toolcall listener");

    // When submitting a goal over the new stdio-RPC transport
    let request = json!({
        "type": "submit",
        "goal": "plan",
        "data": {"prd": "# minimal"},
    });
    let response = tokio::time::timeout(CALL_TIMEOUT, dispatch_toolcall(&socket_path, request))
        .await
        .expect("submit relay timed out")
        .expect("submit relay succeeds");

    // Then the relay acknowledges the submit immediately
    assert_eq!(response["status"], "ok");
    assert_eq!(response["goal"], "plan");
}
