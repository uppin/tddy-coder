//! Acceptance: `build` toolcall verb relayed over the new `tddy-rpc`/`tddy-stdio` framing — see
//! `toolcall_stdio_relay_submit_acceptance.rs` for the migration context. One test per file:
//! `start_toolcall_listener`'s socket path is keyed only by process id.

use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tddy_core::toolcall::{start_toolcall_listener, BuildExecutor, BuildListQuery, BuildOptions};
use tddy_tools::toolcall_client::dispatch_toolcall;

const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// Fixed-response `BuildExecutor` — enough to prove a `build` request round-trips through the
/// listener's registered-executor extension point over the new transport, without depending on a
/// real `tddy-build` invocation.
struct FakeBuildExecutor;

impl BuildExecutor for FakeBuildExecutor {
    fn build_list(
        &self,
        _repo_dir: &Path,
        _query: &BuildListQuery,
    ) -> Result<serde_json::Value, String> {
        Ok(json!({"targets": [], "total": 0}))
    }

    fn build(
        &self,
        _repo_dir: &Path,
        target: &str,
        _opts: &BuildOptions,
    ) -> Result<serde_json::Value, String> {
        Ok(json!({"target": target, "actions": ["stdio-relay-build-ok"]}))
    }
}

/// **build_round_trips_over_the_stdio_rpc_transport**: a `build` request served by the
/// listener's registered `BuildExecutor` round-trips over the new stdio-RPC transport with the
/// same JSON payload the old line-JSON protocol relays.
#[tokio::test]
async fn build_round_trips_over_the_stdio_rpc_transport() {
    // Given a real toolcall listener with a fake build executor registered
    tddy_core::toolcall::register_build_executor(Arc::new(FakeBuildExecutor));
    let tddy_data_dir =
        std::env::temp_dir().join(format!("tddy-toolcall-stdio-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tddy_data_dir).unwrap();
    let (socket_path, _tool_rx) =
        start_toolcall_listener(None, None, tddy_data_dir).expect("start toolcall listener");

    // When building a target over the new stdio-RPC transport
    let response = tokio::time::timeout(
        CALL_TIMEOUT,
        dispatch_toolcall(
            &socket_path,
            json!({
                "type": "build",
                "repo_dir": "/repo",
                "target": "pkg:bin",
                "no_cache": false,
                "dry_run": false,
            }),
        ),
    )
    .await
    .expect("build relay timed out")
    .expect("build relay succeeds");

    // Then the fake executor's payload comes back over the new transport, status defaulted to ok
    assert_eq!(response["status"], "ok");
    assert_eq!(response["target"], "pkg:bin");
    assert_eq!(response["actions"], json!(["stdio-relay-build-ok"]));
}
