# 2026-10-23 — workspace/executeCommand with applyEdit capture, and pulled diagnostics (`#reshape` 23/24, #682)

**Type:** Feature

`LspClient::execute_command` → `ExecutedCommand { result, applied_edits }`: every `workspace/applyEdit` the server sends during the call is captured and answered `{applied: true}` (one with no `edit` is answered `applied: false` and not kept), one command at a time per client (`command_gate`). `pull_diagnostics` sends `textDocument/diagnostic`. `fake_lsp` gains `--code-actions` and `--diagnoses`. See [commands-and-diagnostics.md](../commands-and-diagnostics.md).

Full history: [cross-package entry](../../../../docs/dev/changesets/2026-10-23-reshape-extension-code-actions.md).
