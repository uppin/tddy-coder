# Changesets Applied

Wrapped changeset history for tddy-lsp.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-07-22** [Feature] reusable-lsp: new crate — allow-list + `language_for_target_type`; JSON-RPC `LspClient` over `tddy-task` channels (definition/references/hover/symbols/diagnostics + `workspace/diagnostic`, id correlation, `publishDiagnostics` cache, server→client request replies); long-running `LspServerBody` (`TaskBody`); per-`(root, language)` `LspRegistry` with lazy get-or-spawn, `IdleTimeoutTracker`-based idle teardown, and respawn-after-crash. Dep `lsp-types` (types only). Cross-package [changeset](../../../docs/dev/changesets.md). PR [#310](https://github.com/uppin/tddy-coder/pull/310).
