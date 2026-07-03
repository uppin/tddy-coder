//! Acceptance: `list-actions`/`invoke-action` toolcall verbs relayed over the new
//! `tddy-rpc`/`tddy-stdio` framing — see `toolcall_stdio_relay_submit_acceptance.rs` for the
//! migration context. One test per file: `start_toolcall_listener`'s socket path is keyed only by
//! process id.

use serde_json::json;
use std::time::Duration;
use tddy_core::toolcall::start_toolcall_listener;
use tddy_tools::toolcall_client::dispatch_toolcall;

const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// **list_actions_and_invoke_action_round_trip_over_the_stdio_rpc_transport**: `list-actions` and
/// `invoke-action` are handled directly in the listener (no presenter involved); both must
/// round-trip over the new stdio-RPC transport with the same response shape as today.
#[tokio::test]
async fn list_actions_and_invoke_action_round_trip_over_the_stdio_rpc_transport() {
    // Given a real toolcall listener scoped to an empty repo (no actions defined)
    let repo = tempfile::tempdir().unwrap();
    let tddy_data_dir =
        std::env::temp_dir().join(format!("tddy-toolcall-stdio-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tddy_data_dir).unwrap();
    let (socket_path, _tool_rx) =
        start_toolcall_listener(None, Some(repo.path().to_path_buf()), tddy_data_dir)
            .expect("start toolcall listener");

    // When listing actions over the new stdio-RPC transport
    let list_response = tokio::time::timeout(
        CALL_TIMEOUT,
        dispatch_toolcall(
            &socket_path,
            json!({"type": "list-actions", "path_prefix": null, "query": null}),
        ),
    )
    .await
    .expect("list-actions relay timed out")
    .expect("list-actions relay succeeds");

    // Then the empty repo yields zero actions, the same shape the old protocol returns
    assert_eq!(list_response["status"], "ok");
    assert_eq!(list_response["total"], 0);
    assert_eq!(list_response["actions"], json!([]));

    // When invoking a non-existent action over the new stdio-RPC transport
    let invoke_response = tokio::time::timeout(
        CALL_TIMEOUT,
        dispatch_toolcall(
            &socket_path,
            json!({
                "type": "invoke-action",
                "action": "does-not-exist",
                "data": "{}",
            }),
        ),
    )
    .await
    .expect("invoke-action relay timed out")
    .expect("invoke-action relay succeeds");

    // Then the relay reports the failure over the new transport, same as today
    assert_eq!(invoke_response["status"], "error");
}
