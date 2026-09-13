//! The single, language-agnostic LSP MCP tool set.
//!
//! Once a language server is available for the session (signalled by the `TDDY_LSP_TOOLS`
//! env gate set by the owner), these five tools are merged into `tddy-tools`' MCP server and
//! dispatched over the session-tool transport to the owner's [`crate::TddyLspExecutor`]. The names
//! carry no language prefix — one interface serves every language.
//!
//! The catalog lives here rather than in `tddy-tools` because this crate is what answers the calls:
//! the operations and their argument shapes are LSP-executor facts. How they are dressed up as MCP
//! tool definitions stays in the crate that speaks MCP.

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

/// One LSP operation as it is offered to an agent: what it is called, what it does, and the
/// arguments it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema_json: &'static str,
}

/// The LSP tool catalog: one language-agnostic def per operation.
pub fn lsp_tool_catalog() -> Vec<LspToolDef> {
    const POSITION_SCHEMA: &str = r#"{"type":"object","required":["target","file","line","character"],"properties":{"target":{"type":"string"},"file":{"type":"string"},"line":{"type":"integer"},"character":{"type":"integer"}}}"#;

    vec![
        LspToolDef {
            name: "LspDiagnostics",
            description: "List language-server diagnostics for a file in a build target.",
            input_schema_json: r#"{"type":"object","required":["target","file"],"properties":{"target":{"type":"string"},"file":{"type":"string"}}}"#,
        },
        LspToolDef {
            name: "LspDefinition",
            description: "Go to the definition of the symbol at a file position.",
            input_schema_json: POSITION_SCHEMA,
        },
        LspToolDef {
            name: "LspReferences",
            description: "Find references to the symbol at a file position.",
            input_schema_json: POSITION_SCHEMA,
        },
        LspToolDef {
            name: "LspHover",
            description: "Show hover information for the symbol at a file position.",
            input_schema_json: POSITION_SCHEMA,
        },
        LspToolDef {
            name: "LspSymbols",
            description: "Search workspace symbols for a build target.",
            input_schema_json: r#"{"type":"object","required":["target"],"properties":{"target":{"type":"string"},"query":{"type":"string"}}}"#,
        },
    ]
}

/// Whether the LSP tools should be exposed for this session (the `TDDY_LSP_TOOLS` gate).
///
/// A blank value counts as unset: an outer process exporting the variable empty has not made a
/// language server available.
pub fn lsp_tools_enabled() -> bool {
    std::env::var(LSP_TOOLS_ENV).is_ok_and(|value| !value.trim().is_empty())
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
        let names: Vec<&str> = catalog.iter().map(|t| t.name).collect();

        // Then it is exactly the five language-agnostic operations, with no language prefix
        assert_eq!(names, LSP_TOOL_NAMES);
        assert!(
            names
                .iter()
                .all(|n| !n.to_ascii_lowercase().contains("rust")),
            "LSP tool names must not name a language"
        );
    }
}
