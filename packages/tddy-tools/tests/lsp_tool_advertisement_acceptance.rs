//! Acceptance tests: the MCP server advertises the five language-agnostic `Lsp*` tools exactly
//! when the owner signalled, via `TDDY_LSP_TOOLS`, that a language server is available.
//!
//! These lived beside the catalog in `tddy-tools`' `lsp_tools` module. `#unbundle` node 5 moved the
//! catalog to `tddy-lsp-executor` — the crate that answers the calls — but what these two assert is
//! a `tddy-tools` fact: that *this* crate's `PermissionServer` merges the catalog into its router
//! behind the gate. `PermissionServer` stays here, so they do too.
//!
//! The gate is process-wide environment, so they are `#[serial]` against each other.

use serial_test::serial;
use tddy_lsp_executor::lsp_tools::{LSP_TOOLS_ENV, LSP_TOOL_NAMES};
use tddy_tools::server::PermissionServer;

#[test]
#[serial]
fn the_mcp_server_exposes_the_lsp_tools_when_the_gate_is_set() {
    // Given a session where a language server is available
    std::env::set_var(LSP_TOOLS_ENV, "rust");

    // When the MCP server is built
    let names = PermissionServer::new().tool_names();
    std::env::remove_var(LSP_TOOLS_ENV);

    // Then all five LSP tools are advertised
    for tool in LSP_TOOL_NAMES {
        assert!(
            names.contains(&tool.to_string()),
            "expected {tool} to be exposed"
        );
    }
}

#[test]
#[serial]
fn the_mcp_server_hides_the_lsp_tools_without_the_gate() {
    // Given a session with no language server available
    std::env::remove_var(LSP_TOOLS_ENV);

    // When the MCP server is built
    let names = PermissionServer::new().tool_names();

    // Then no LSP tool is advertised
    for tool in LSP_TOOL_NAMES {
        assert!(
            !names.contains(&tool.to_string()),
            "expected {tool} to be hidden"
        );
    }
}
