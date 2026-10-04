# 2026-10-04 — `index_backed`: the executor that asks the warm index

**Type:** Feature

`#live-plan` 11/15, PR [#570](https://github.com/uppin/tddy-coder/pull/570). Cross-package entry:
[2026-10-04-session-lsp-tools.md](../../../../docs/dev/changesets/2026-10-04-session-lsp-tools.md).

`src/index_backed.rs` adds `IndexChannel` (an async port for a channel to the index),
`IndexLspExecutor` (an `LspExecutor` whose five tools ask `code_index.CodeIndexService`),
`bind_to_session_worktree` (the host-side refusal of a file outside the session's worktree) and
`select_lsp_executor` (the deployment switch). Positions are converted between the tools' UTF-16 columns
and the index's byte columns by reading the line from disk, in both directions. `ReadLints` is refused.
See [index-backed-executor.md](../index-backed-executor.md).

`tddy-index-daemon`, `tonic` and `async-trait` become `[dependencies]` entries, so every dependent of
this crate links the index daemon's library; there is no cycle.

Tests: `tests/lsp_tools_via_index.rs` against a fake `code_index` server on a socket, and the binding's
unit tests. They found two defects in the binding, both fixed: `"."` was accepted as a file, and
`"./src/lib.rs"` was bound with its leading `./`.
