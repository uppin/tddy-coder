//! Acceptance tests: the static exec-tool catalog and the pure catalog-to-router builder
//! (PRD: docs/ft/daemon/remote-codebase-mode.md, AC15/AC17).
//!
//! `dynamic_tool_router(catalog)` is the seam between a resolved tool catalog and the live
//! MCP `ToolRouter<PermissionServer>` that `PermissionServer::new()` merges into its
//! macro-generated static router. It is deliberately pure (catalog in, router out) so it can be
//! tested with arbitrary catalogs without touching process env vars. The env-var-driven decision
//! of *whether* to merge it (based on `detect_session_tool_transport()`) is covered separately in
//! `dynamic_tool_router_wiring_acceptance.rs`, via `PermissionServer::tool_names()`.
//!
//! See `mcp_stdio_dynamic_tools_acceptance.rs` for the real MCP-over-stdio proof that this
//! router is actually wired into `tddy-tools --mcp`.

use std::collections::HashSet;

use tddy_tools::server::{dynamic_tool_router, exec_tool_catalog};

/// AC15: the static exec-tool catalog names every cursor tool the sandbox remote-codebase
/// appendix (`SANDBOX_REMOTE_APPENDIX` in tddy-sandbox) promises Claude it can use.
#[test]
fn exec_tool_catalog_names_every_documented_cursor_tool() {
    // Given / When
    let catalog = exec_tool_catalog();
    let names: HashSet<&str> = catalog.iter().map(|t| t.name.as_str()).collect();

    // Then
    let expected: HashSet<&str> = [
        "Read",
        "Write",
        "StrReplace",
        "Delete",
        "Grep",
        "Glob",
        "Shell",
        "Await",
        "ReadLints",
        "SemanticSearch",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        names, expected,
        "exec_tool_catalog must name exactly the documented cursor tools"
    );
}

/// AC15: every catalog entry carries a valid JSON-object input schema — `build_dynamic_tool_list`
/// rejects entries whose `input_schema_json` doesn't parse as a JSON object.
#[test]
fn exec_tool_catalog_entries_have_valid_object_schemas() {
    // Given
    let catalog = exec_tool_catalog();

    // When / Then
    for def in &catalog {
        let parsed: serde_json::Value = serde_json::from_str(&def.input_schema_json)
            .unwrap_or_else(|e| panic!("'{}' input_schema_json must be valid JSON: {e}", def.name));
        assert!(
            parsed.is_object(),
            "'{}' input_schema_json must be a JSON object, got: {}",
            def.name,
            def.input_schema_json
        );
    }
}

/// AC17: the router built from the real exec-tool catalog never registers the
/// statically-handled tool names — those stay owned by the macro-generated router that
/// `PermissionServer::new()` merges this one into. Deterministic: no process env vars involved.
#[test]
fn dynamic_tool_router_built_from_exec_catalog_never_registers_static_tool_names() {
    // Given
    let catalog = exec_tool_catalog();

    // When
    let router = dynamic_tool_router(&catalog);

    // Then
    for static_name in [
        "approval_prompt",
        "github_create_pull_request",
        "github_update_pull_request",
    ] {
        assert!(
            !router.has_route(static_name),
            "dynamic router must not register the static tool '{static_name}'"
        );
    }
}
