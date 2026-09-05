# 2026-07-22 — reusable-lsp: `execute_tool` dispatches the five `Lsp*` tools + workspace-level `ReadLints` to `tddy_core::toolcall::lsp::lsp_executor()` (new `tddy-core` dep); `ReadLints` falls back to the no-linter stub when no executor is registered. Cross-package [changeset](../../../../docs/dev/changesets/). PR [#310](https://github.com/uppin/tddy-coder/pull/310).

**Type:** Feature


