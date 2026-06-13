//! Acceptance tests: tddy-tools relay dispatch — Phase 4 (PRD: docs/ft/daemon/remote-codebase-mode.md).
//!
//! AC15/AC18/AC19: when TDDY_REMOTE_* env vars are set and a relay is reachable,
//! `dispatch_dynamic_tool` must forward the call to the relay's ExecuteTool RPC and return
//! the relay's response — not the "relay not yet implemented" stub error.
//!
//! AC (hard-deny): when TDDY_REMOTE_SESSION_ID is set, the MCP approval handler must
//! hard-deny native fs/shell tools (Write, Edit, NotebookEdit) even if they are not in the
//! allowlist — ensuring an agent in remote mode cannot accidentally write local files.

use serde_json::{json, Value};
use std::collections::HashSet;

/// Verify that `dispatch_dynamic_tool` does NOT return the stub "not yet implemented" message
/// when `TDDY_REMOTE_SESSION_ID` and `TDDY_REMOTE_DAEMON_URL` are set.
///
/// The stub currently always returns `{"error": "tool '...' relay not yet implemented"}`.
/// After Phase 4, when env vars are set it must attempt an actual HTTP call to the relay.
/// (The call will fail if no relay is listening, but it must fail with a connection/RPC error —
/// not the hardcoded "not yet implemented" string.)
#[tokio::test]
async fn dispatch_dynamic_tool_does_not_return_stub_error_when_env_set() {
    // Use a port that is very unlikely to have a listener — we expect a connection error,
    // NOT the hardcoded "relay not yet implemented" stub.
    std::env::set_var("TDDY_REMOTE_SESSION_ID", "test-session-dispatch-123");
    std::env::set_var("TDDY_REMOTE_DAEMON_URL", "http://127.0.0.1:19731");

    let result =
        tddy_tools::server::dispatch_dynamic_tool("Read", json!({"path": "src/main.rs"})).await;

    std::env::remove_var("TDDY_REMOTE_SESSION_ID");
    std::env::remove_var("TDDY_REMOTE_DAEMON_URL");

    let parsed: Value =
        serde_json::from_str(&result).expect("dispatch_dynamic_tool must return valid JSON");

    // The stub returns this exact string — after Phase 4, this message must NOT appear.
    let error_msg = parsed.get("error").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        !error_msg.contains("relay not yet implemented"),
        "dispatch_dynamic_tool must attempt a real relay call when env vars are set; \
         got stub error: {}",
        result
    );
}

/// Verify that `is_native_tool_denied_in_remote_mode` returns true for native write tools.
///
/// Phase 4 adds this public helper so both the approval_prompt handler and tests can check
/// whether a tool must be hard-denied when the agent is in remote mode.
#[test]
fn is_native_tool_denied_in_remote_mode_covers_write_edit_notebook() {
    let must_deny = ["Write", "Edit", "NotebookEdit"];
    let must_allow = ["approval_prompt", "submit", "AskUserQuestion"];

    for tool in &must_deny {
        assert!(
            tddy_tools::server::is_native_tool_denied_in_remote_mode(tool),
            "'{}' must be classified as denied in remote mode",
            tool
        );
    }

    for tool in &must_allow {
        assert!(
            !tddy_tools::server::is_native_tool_denied_in_remote_mode(tool),
            "'{}' must NOT be classified as denied in remote mode",
            tool
        );
    }
}

/// Verify `build_dynamic_tool_list` does NOT include any native fs/shell tools by default —
/// the names must come exclusively from the daemon catalog passed in.
#[tokio::test]
async fn build_dynamic_tool_list_does_not_inject_native_tools() {
    let catalog = vec![tddy_tools::server::RemoteToolDef {
        name: "Read".to_string(),
        description: "Remote read".to_string(),
        input_schema_json: r#"{"type":"object"}"#.to_string(),
    }];

    let tools = tddy_tools::server::build_dynamic_tool_list(&catalog)
        .await
        .expect("build must not fail");

    let names: HashSet<String> = tools.iter().map(|t| t.name.to_string()).collect();

    // These native tool names must NOT appear unless explicitly in the catalog.
    let forbidden_native = ["Bash", "Edit", "NotebookEdit", "WebFetch", "WebSearch"];
    for native in &forbidden_native {
        assert!(
            !names.contains(*native),
            "native tool '{}' must not appear in dynamic tool list; got names: {:?}",
            native,
            names
        );
    }
}
