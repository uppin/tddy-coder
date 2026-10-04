# 2026-10-04 — `ReadLints` is refused when the session's tools are answered by the warm index

**Category:** Deferred feature
**Source:** `#live-plan` 11/15, [#570](https://github.com/uppin/tddy-coder/pull/570), `/pr-wrap` production-readiness check.

## What is left

With an `index_daemon:` section configured, a session's `Lsp*` tools are answered by the warm index
(`tddy_lsp_executor::index_backed::IndexLspExecutor`). `ReadLints` (`LspExecutor::workspace_diagnostics`)
is **refused** there, with a message pointing the agent at `LspDiagnostics` with a file:

> ReadLints is not available through the warm index: it reports diagnostics one file at a time — use
> LspDiagnostics with a file

Without `index_daemon:` it works as before, so the tool's availability depends on a deployment setting.
The marker is `TODO(docs/dev/todo/2026-10-04-read-lints-is-refused-through-the-warm-index.md)` in
`packages/tddy-lsp-executor/src/index_backed.rs`.

## Why it was left

`code_index.CodeIndexService.Diagnostics` answers for **one file** and has no workspace-wide form.
Answering `ReadLints` from the index needs a contract decision — an empty `file` meaning "the whole
workspace", or a separate RPC — which is the index daemon's wire format, not a detail of this node.
Answering it from the executor's own language server instead would be a silent fallback: a second
server that disagrees with the index every other pane asks, so the node refuses rather than falls back.

The refusal is pinned by
`read_lints_is_refused_through_the_index_and_not_answered_by_the_existing_executor`.

## What would close it

A workspace-wide `Diagnostics` on the index (decided: empty `file`, or a new RPC), served by
`tddy-index-daemon/src/symbols.rs`, and `workspace_diagnostics` answering through it. Then the refusal
and its test are replaced by a test that `ReadLints` is answered by the index.
