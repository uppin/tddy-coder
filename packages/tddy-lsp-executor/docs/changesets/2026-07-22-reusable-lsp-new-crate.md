# 2026-07-22 — reusable-lsp: new crate

**Type:** Feature

`TddyLspExecutor` implements `tddy_core::toolcall::lsp::LspExecutor` over `tddy-lsp` + `tddy-build` target discovery (target id → `config.type` → `Language` → allow-list → `LspKey{workspace_root, language}`), plus `workspace_diagnostics` for `ReadLints`; `register(task_registry, allow, idle_timeout)` registers the process-global executor and returns the `LspRegistry` for an idle reaper. Registered by the daemon and sandbox-app. Cross-package [changeset](../../../../docs/dev/changesets/). PR [#310](https://github.com/uppin/tddy-coder/pull/310).
