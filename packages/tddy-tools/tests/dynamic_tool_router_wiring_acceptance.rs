//! Acceptance tests: wiring the dynamic tool catalog into `PermissionServer`'s live MCP
//! `ServerHandler` (PRD: docs/ft/daemon/remote-codebase-mode.md, AC15-19).
//!
//! `relay_dispatch_acceptance.rs` and `remote_mcp_proxy_acceptance.rs` already cover
//! `build_dynamic_tool_list`/`dispatch_dynamic_tool` as standalone functions called directly.
//! Neither exercises the actual router that `tools/list`/`call_tool` read from at runtime — that
//! seam (construction-time merge into `PermissionServer::tool_router`) is what these tests cover.

use std::collections::HashSet;

use serial_test::serial;
use tddy_tools::server::{dynamic_tool_router, exec_tool_catalog, PermissionServer, RemoteToolDef};

fn clear_session_tool_transport_env() {
    std::env::remove_var("TDDY_SANDBOX_TOOL_IPC");
    std::env::remove_var("TDDY_REMOTE_SESSION_ID");
    std::env::remove_var("TDDY_REMOTE_DAEMON_URL");
}

/// AC15: when a sandbox IPC transport is configured, the live MCP tool list (what
/// `tools/list` will actually report) must include every workspace exec tool.
#[test]
#[serial]
fn permission_server_exposes_exec_tool_names_when_sandbox_ipc_configured() {
    // Given — a sandbox IPC socket path is configured, as it would be inside the jail.
    clear_session_tool_transport_env();
    std::env::set_var("TDDY_SANDBOX_TOOL_IPC", "/tmp/tddy-wiring-test-ipc.sock");

    // When
    let server = PermissionServer::new();
    let names: HashSet<String> = server.tool_names().into_iter().collect();

    clear_session_tool_transport_env();

    // Then
    for exec_tool in tddy_sandbox::workspace_exec_tool_names() {
        assert!(
            names.contains(*exec_tool),
            "PermissionServer must expose '{}' via tools/list when TDDY_SANDBOX_TOOL_IPC is set; got: {:?}",
            exec_tool,
            names
        );
    }
}

/// AC15 (HTTP relay path): same requirement, but for the daemon-HTTP transport used when
/// there is no sandbox IPC socket (e.g. `tddy-sandbox-app --remote-codebase`'s host relay).
#[test]
#[serial]
fn permission_server_exposes_exec_tool_names_when_remote_daemon_configured() {
    // Given
    clear_session_tool_transport_env();
    std::env::set_var("TDDY_REMOTE_SESSION_ID", "wiring-test-session");
    std::env::set_var("TDDY_REMOTE_DAEMON_URL", "http://127.0.0.1:19999");

    // When
    let server = PermissionServer::new();
    let names: HashSet<String> = server.tool_names().into_iter().collect();

    clear_session_tool_transport_env();

    // Then
    for exec_tool in tddy_sandbox::workspace_exec_tool_names() {
        assert!(
            names.contains(*exec_tool),
            "PermissionServer must expose '{}' via tools/list when TDDY_REMOTE_* is set; got: {:?}",
            exec_tool,
            names
        );
    }
}

/// AC19: without any session-tool transport configured, the live tool list must NOT
/// advertise exec tools that have nowhere to dispatch to.
#[test]
#[serial]
fn permission_server_omits_exec_tool_names_without_any_transport_configured() {
    // Given
    clear_session_tool_transport_env();

    // When
    let server = PermissionServer::new();
    let names: HashSet<String> = server.tool_names().into_iter().collect();

    // Then
    for exec_tool in tddy_sandbox::workspace_exec_tool_names() {
        assert!(
            !names.contains(*exec_tool),
            "PermissionServer must NOT expose '{}' via tools/list without a session-tool transport configured; got: {:?}",
            exec_tool,
            names
        );
    }
}

/// AC17: statically-registered MCP tools remain present even after dynamic tools are merged
/// in — the merge must be additive, never a replacement.
#[test]
#[serial]
fn permission_server_still_exposes_static_tools_when_sandbox_ipc_configured() {
    // Given
    clear_session_tool_transport_env();
    std::env::set_var("TDDY_SANDBOX_TOOL_IPC", "/tmp/tddy-wiring-test-static.sock");

    // When
    let server = PermissionServer::new();
    let names: HashSet<String> = server.tool_names().into_iter().collect();

    clear_session_tool_transport_env();

    // Then
    for static_tool in [
        "approval_prompt",
        "github_create_pull_request",
        "github_update_pull_request",
    ] {
        assert!(
            names.contains(static_tool),
            "'{}' must remain in tools/list even when dynamic tools are merged in; got: {:?}",
            static_tool,
            names
        );
    }
}

/// Guards against `exec_tool_catalog()` (the tddy-tools-side schema catalog) drifting from
/// `workspace_exec_tool_names()` (the canonical name list already used to build the
/// sandboxed Claude CLI's `--allowedTools`) — mirrors the equivalent sync check in
/// `tddy_daemon::tool_catalog`.
#[test]
fn exec_tool_catalog_names_match_workspace_exec_tool_names() {
    // Given / When
    let catalog_names: HashSet<String> = exec_tool_catalog()
        .into_iter()
        .map(|def| def.name)
        .collect();
    let sandbox_names: HashSet<String> = tddy_sandbox::workspace_exec_tool_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

    // Then
    assert_eq!(
        catalog_names, sandbox_names,
        "exec_tool_catalog() must stay in sync with workspace_exec_tool_names()"
    );
}

/// AC16: the router built from a catalog contains exactly the given entries — renaming a
/// catalog entry must be reflected 1:1 in what the router (and therefore `tools/list`) exposes.
#[test]
fn dynamic_tool_router_exposes_exactly_the_given_catalog_entries() {
    // Given
    let catalog = vec![
        RemoteToolDef {
            name: "AlphaTool".to_string(),
            description: "First custom tool".to_string(),
            input_schema_json: r#"{"type":"object"}"#.to_string(),
        },
        RemoteToolDef {
            name: "BetaTool".to_string(),
            description: "Second custom tool".to_string(),
            input_schema_json: r#"{"type":"object"}"#.to_string(),
        },
    ];

    // When
    let router = dynamic_tool_router(&catalog);
    let names: HashSet<String> = router
        .list_all()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();

    // Then
    let expected: HashSet<String> = ["AlphaTool", "BetaTool"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(
        names, expected,
        "router must contain exactly the catalog entries, got: {:?}",
        names
    );
}

/// AC18 (prerequisite): the route registered for a catalog entry carries that entry's exact
/// description and input schema, so a real `call_tool` against it validates arguments and
/// reports capabilities using the daemon's schema — not a placeholder.
#[test]
fn dynamic_tool_router_preserves_catalog_description_and_schema() {
    // Given
    let catalog = vec![RemoteToolDef {
        name: "ReadFileTool".to_string(),
        description: "Read a file from the remote workspace.".to_string(),
        input_schema_json:
            r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"}}}"#
                .to_string(),
    }];

    // When
    let router = dynamic_tool_router(&catalog);
    let tool = router
        .get("ReadFileTool")
        .cloned()
        .expect("ReadFileTool route must be registered");

    // Then
    assert_eq!(
        tool.description.as_deref(),
        Some("Read a file from the remote workspace."),
        "dynamic route must carry the catalog description"
    );
    assert_eq!(
        tool.input_schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(1),
        "dynamic route must carry the catalog input schema"
    );
}
