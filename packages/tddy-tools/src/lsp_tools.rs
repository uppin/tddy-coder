//! The single, language-agnostic LSP MCP tool set.
//!
//! Once a language server is available for the session (signalled by the `TDDY_LSP_TOOLS`
//! env gate set by the owner), these five tools are merged into the MCP server in
//! [`crate::server::PermissionServer::new`] and dispatched over the session-tool transport
//! to the owner's `LspExecutor`. The names carry no language prefix — one interface serves
//! every language.

use crate::server::{env_non_empty, RemoteToolDef};

/// Env var the owner sets per session when ≥1 language server is available. Its presence
/// (non-empty) gates whether the LSP tools are exposed to the agent.
pub const LSP_TOOLS_ENV: &str = "TDDY_LSP_TOOLS";

/// The five language-agnostic tool names, in catalog order.
pub const LSP_TOOL_NAMES: [&str; 5] = [
    "LspDiagnostics",
    "LspDefinition",
    "LspReferences",
    "LspHover",
    "LspSymbols",
];

/// The LSP tool catalog: one language-agnostic def per operation.
pub fn lsp_tool_catalog() -> Vec<RemoteToolDef> {
    const POSITION_SCHEMA: &str = r#"{"type":"object","required":["target","file","line","character"],"properties":{"target":{"type":"string"},"file":{"type":"string"},"line":{"type":"integer"},"character":{"type":"integer"}}}"#;

    vec![
        RemoteToolDef {
            name: "LspDiagnostics".to_string(),
            description: "List language-server diagnostics for a file in a build target."
                .to_string(),
            input_schema_json:
                r#"{"type":"object","required":["target","file"],"properties":{"target":{"type":"string"},"file":{"type":"string"}}}"#
                    .to_string(),
        },
        RemoteToolDef {
            name: "LspDefinition".to_string(),
            description: "Go to the definition of the symbol at a file position.".to_string(),
            input_schema_json: POSITION_SCHEMA.to_string(),
        },
        RemoteToolDef {
            name: "LspReferences".to_string(),
            description: "Find references to the symbol at a file position.".to_string(),
            input_schema_json: POSITION_SCHEMA.to_string(),
        },
        RemoteToolDef {
            name: "LspHover".to_string(),
            description: "Show hover information for the symbol at a file position.".to_string(),
            input_schema_json: POSITION_SCHEMA.to_string(),
        },
        RemoteToolDef {
            name: "LspSymbols".to_string(),
            description: "Search workspace symbols for a build target.".to_string(),
            input_schema_json:
                r#"{"type":"object","required":["target"],"properties":{"target":{"type":"string"},"query":{"type":"string"}}}"#
                    .to_string(),
        },
    ]
}

/// Whether the LSP tools should be exposed for this session (the `TDDY_LSP_TOOLS` gate).
pub fn lsp_tools_enabled() -> bool {
    env_non_empty(LSP_TOOLS_ENV).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn the_five_lsp_tools_are_absent_without_the_availability_gate() {
        // Given no LSP language server is available for the session
        std::env::remove_var(LSP_TOOLS_ENV);

        // When the gate is evaluated
        let enabled = lsp_tools_enabled();

        // Then the LSP tools are not exposed
        assert!(
            !enabled,
            "expected LSP tools to be gated off when unavailable"
        );
    }

    #[test]
    #[serial]
    fn the_five_lsp_tools_are_present_behind_the_availability_gate() {
        // Given a language server is available for the session
        std::env::set_var(LSP_TOOLS_ENV, "rust");

        // When the gate is evaluated
        let enabled = lsp_tools_enabled();
        std::env::remove_var(LSP_TOOLS_ENV);

        // Then the LSP tools are exposed
        assert!(
            enabled,
            "expected LSP tools to be exposed when a server is available"
        );
    }

    #[test]
    fn lsp_tool_names_are_language_agnostic() {
        // Given the LSP tool catalog
        let catalog = lsp_tool_catalog();

        // When we read the tool names
        let names: Vec<&str> = catalog.iter().map(|t| t.name.as_str()).collect();

        // Then it is exactly the five language-agnostic operations, with no language prefix
        assert_eq!(names, LSP_TOOL_NAMES);
        assert!(
            names
                .iter()
                .all(|n| !n.to_ascii_lowercase().contains("rust")),
            "LSP tool names must not name a language"
        );
    }

    #[test]
    #[serial]
    fn the_mcp_server_exposes_the_lsp_tools_when_the_gate_is_set() {
        // Given a session where a language server is available
        std::env::set_var(LSP_TOOLS_ENV, "rust");

        // When the MCP server is built
        let names = crate::server::PermissionServer::new().tool_names();
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
        let names = crate::server::PermissionServer::new().tool_names();

        // Then no LSP tool is advertised
        for tool in LSP_TOOL_NAMES {
            assert!(
                !names.contains(&tool.to_string()),
                "expected {tool} to be hidden"
            );
        }
    }
}
