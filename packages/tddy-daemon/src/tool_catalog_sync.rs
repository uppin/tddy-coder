//! Cross-check that the sandbox claude exec-tool allowlist stays in sync with the shared tool
//! catalog exposed via `ListExecTools` / `ExecuteTool`.
//!
//! The catalog itself lives in the `tddy-tool-engine` crate (shared with `tddy-coder`); this test
//! guards the daemon-specific invariant that `tddy-sandbox`'s allowlist covers every tool the
//! engine dispatches.

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    #[test]
    fn workspace_exec_tool_names_match_tool_catalog() {
        // Given
        let catalog: HashSet<String> = crate::tool_engine::tool_catalog()
            .into_iter()
            .map(|t| t.name)
            .collect();
        let sandbox: HashSet<&str> = tddy_sandbox::workspace_exec_tool_names()
            .iter()
            .copied()
            .collect();

        // Then — sandbox claude allowlist must cover the same exec tools as ListExecTools
        assert_eq!(
            catalog,
            sandbox
                .into_iter()
                .map(String::from)
                .collect::<HashSet<_>>(),
            "workspace_exec_tool_names must stay in sync with the shared tool_catalog"
        );
    }
}
