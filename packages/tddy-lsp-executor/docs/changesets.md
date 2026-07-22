# Changesets Applied

Wrapped changeset history for tddy-lsp-executor.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-07-22** [Feature] reusable-lsp: new crate — `TddyLspExecutor` implements `tddy_core::toolcall::lsp::LspExecutor` over `tddy-lsp` + `tddy-build` target discovery (target id → `config.type` → `Language` → allow-list → `LspKey{workspace_root, language}`), plus `workspace_diagnostics` for `ReadLints`; `register(task_registry, allow, idle_timeout)` registers the process-global executor and returns the `LspRegistry` for an idle reaper. Registered by the daemon and sandbox-app. Cross-package [changeset](../../../docs/dev/changesets.md). PR [#310](https://github.com/uppin/tddy-coder/pull/310).
