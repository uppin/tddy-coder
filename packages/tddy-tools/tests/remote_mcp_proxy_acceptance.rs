//! Acceptance tests: tddy-tools dynamic MCP proxy (PRD: docs/ft/daemon/remote-codebase-mode.md).
//!
//! AC15-AC19: list_tools advertises the daemon's catalog (not a hardcoded set); approval_prompt
//! and submit remain static; call_tool for unknown names forwards to relay; missing env → error.
//!
//! These tests invoke the MCP server layer directly via the library, not via the subprocess
//! boundary, so they can inspect the ServerHandler behaviour without a full Claude session.

use std::collections::HashSet;

use serde_json::Value;
use tddy_tools::server::{build_dynamic_tool_list, static_tool_names};

/// AC15 (partial): `build_dynamic_tool_list` with a mock catalog merges the catalog into the
/// static tools and returns exactly the union — no extra, no missing.
///
/// This tests the merging logic independently of the relay transport.
#[tokio::test]
async fn dynamic_tool_list_merges_catalog_with_static_tools() {
    let catalog = vec![
        tddy_tools::server::RemoteToolDef {
            name: "Read".to_string(),
            description: "Read a file".to_string(),
            input_schema_json: r#"{"type":"object","properties":{"path":{"type":"string"}}}"#
                .to_string(),
        },
        tddy_tools::server::RemoteToolDef {
            name: "Write".to_string(),
            description: "Write a file".to_string(),
            input_schema_json: r#"{"type":"object","properties":{"path":{"type":"string"},"contents":{"type":"string"}}}"#
                .to_string(),
        },
    ];

    let tools = build_dynamic_tool_list(&catalog)
        .await
        .expect("build_dynamic_tool_list must not fail");

    let names: HashSet<String> = tools.iter().map(|t| t.name.to_string()).collect();

    // Static tools must always be present.
    for static_name in static_tool_names() {
        assert!(
            names.contains(static_name),
            "static tool '{}' must always appear in the MCP tool list",
            static_name
        );
    }

    // Each catalog entry must also appear.
    assert!(names.contains("Read"), "catalog tool 'Read' must appear");
    assert!(names.contains("Write"), "catalog tool 'Write' must appear");

    // No extra tools beyond static + catalog.
    let expected: HashSet<String> = static_tool_names()
        .iter()
        .map(|s| s.to_string())
        .chain(vec!["Read".to_string(), "Write".to_string()])
        .collect();
    assert_eq!(
        names,
        expected,
        "tool list must be exactly static + catalog, got extras: {:?}",
        names.difference(&expected).collect::<Vec<_>>()
    );
}

/// AC16: renaming a catalog entry causes the new name to appear and the old name to disappear.
#[tokio::test]
async fn dynamic_tool_list_reflects_catalog_renames() {
    let catalog_v1 = vec![tddy_tools::server::RemoteToolDef {
        name: "OldName".to_string(),
        description: "A tool".to_string(),
        input_schema_json: r#"{"type":"object"}"#.to_string(),
    }];
    let catalog_v2 = vec![tddy_tools::server::RemoteToolDef {
        name: "NewName".to_string(),
        description: "A tool".to_string(),
        input_schema_json: r#"{"type":"object"}"#.to_string(),
    }];

    let tools_v1 = build_dynamic_tool_list(&catalog_v1)
        .await
        .expect("v1 build_dynamic_tool_list must not fail");
    let names_v1: HashSet<String> = tools_v1.iter().map(|t| t.name.to_string()).collect();

    let tools_v2 = build_dynamic_tool_list(&catalog_v2)
        .await
        .expect("v2 build_dynamic_tool_list must not fail");
    let names_v2: HashSet<String> = tools_v2.iter().map(|t| t.name.to_string()).collect();

    assert!(names_v1.contains("OldName"), "v1 must advertise OldName");
    assert!(
        !names_v1.contains("NewName"),
        "v1 must not advertise NewName"
    );

    assert!(names_v2.contains("NewName"), "v2 must advertise NewName");
    assert!(
        !names_v2.contains("OldName"),
        "v2 must not advertise OldName"
    );
}

/// AC17: `approval_prompt` and `submit` are always static — they must appear regardless of the
/// catalog, and their `call_tool` dispatch must be handled locally (not forwarded to a relay).
#[test]
fn static_tool_names_always_includes_approval_prompt_and_submit() {
    let names = static_tool_names();
    assert!(
        names.contains(&"approval_prompt"),
        "approval_prompt must always be in the static tool list: {:?}",
        names
    );
    assert!(
        names.contains(&"submit"),
        "submit must always be in the static tool list: {:?}",
        names
    );
}

/// AC19: when `TDDY_REMOTE_SESSION_ID` is NOT set, `build_dynamic_tool_list` with an empty
/// catalog returns only the static tools (no dynamic tools, no error).
#[tokio::test]
async fn dynamic_tool_list_without_remote_env_returns_only_static_tools() {
    // Ensure the env var is absent.
    std::env::remove_var("TDDY_REMOTE_SESSION_ID");

    let tools = build_dynamic_tool_list(&[])
        .await
        .expect("build_dynamic_tool_list with empty catalog must not fail");

    let names: HashSet<String> = tools.iter().map(|t| t.name.to_string()).collect();
    let expected: HashSet<String> = static_tool_names().iter().map(|s| s.to_string()).collect();

    assert_eq!(
        names,
        expected,
        "without remote env, only static tools must appear; got extra: {:?}",
        names.difference(&expected).collect::<Vec<_>>()
    );
}

/// AC19 (dynamic call side): calling a dynamic tool name when `TDDY_REMOTE_*` is not set must
/// return an error result explaining the missing configuration — not a panic or RPC error.
#[tokio::test]
async fn call_dynamic_tool_without_remote_env_returns_error_result() {
    std::env::remove_var("TDDY_REMOTE_SESSION_ID");
    std::env::remove_var("TDDY_REMOTE_DAEMON_URL");

    // The `dispatch_dynamic_tool` function handles calls for non-static tools.
    // Without remote env vars, it must return an error-shaped JSON, not panic.
    let result =
        tddy_tools::server::dispatch_dynamic_tool("Read", serde_json::json!({"path":"any.txt"}))
            .await;

    let result_value: Value =
        serde_json::from_str(&result).expect("dispatch_dynamic_tool must return valid JSON");

    // The result must set is_error:true.
    assert!(
        result_value
            .get("is_error")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "dispatch_dynamic_tool without remote env must return is_error:true, got: {:?}",
        result_value
    );
    // The result must include an error message.
    assert!(
        result_value.get("error").is_some(),
        "dispatch_dynamic_tool without remote env must include an error message, got: {:?}",
        result_value
    );
}
