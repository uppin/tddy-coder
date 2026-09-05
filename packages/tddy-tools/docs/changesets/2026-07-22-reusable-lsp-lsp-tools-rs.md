# 2026-07-22 — reusable-lsp: `lsp_tools.rs`

**Type:** Feature

language-agnostic `lsp_tool_catalog()` + `lsp_tools_enabled()` (`TDDY_LSP_TOOLS` gate); merged into `PermissionServer::new` via `dynamic_tool_router` behind the gate. Cross-package [changeset](../../../../docs/dev/changesets/). PR [#310](https://github.com/uppin/tddy-coder/pull/310).
