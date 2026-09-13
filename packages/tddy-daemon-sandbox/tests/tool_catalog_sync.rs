//! Cross-check that the sandbox claude exec-tool allowlist stays in sync with the shared tool
//! catalog exposed via `ListExecTools` / `ExecuteTool`.
//!
//! The catalog itself lives in the `tddy-tool-engine` crate (shared with `tddy-coder`); this test
//! guards the invariant that `tddy-sandbox`'s allowlist covers every tool the
//! engine dispatches.
//!
//! `tddy-tools` keeps a verbatim hand-copy of that catalog, and collapsing the two is a later
//! node's work — until then this guard is what stops them drifting apart, which is why
//! `#unbundle` node 3 relocated it out of `tddy-daemon/src/` rather than deleting it.

use std::collections::HashSet;

#[test]
fn workspace_exec_tool_names_match_tool_catalog() {
    // Given the shared catalog the engine dispatches from
    let catalog: HashSet<String> = tddy_tool_engine::tool_catalog()
        .into_iter()
        .map(|t| t.name)
        .collect();

    // When the sandbox's own allowlist is read
    let sandbox: HashSet<String> = tddy_sandbox::workspace_exec_tool_names()
        .iter()
        .map(|name| String::from(*name))
        .collect();

    // Then — the sandbox claude allowlist must cover the same exec tools as ListExecTools
    assert_eq!(
        catalog, sandbox,
        "workspace_exec_tool_names must stay in sync with the shared tool_catalog"
    );
}
