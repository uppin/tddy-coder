# 2026-10-04 — Session `Lsp*` tools answered by the warm index

**Type:** Feature

`#live-plan` 11/15 — PR [#570](https://github.com/uppin/tddy-coder/pull/570),
`feature/live-plan/session-lsp-tools`. Product entry:
[2026-10-04-session-lsp-tools.md](../../ft/coder/changelog/2026-10-04-session-lsp-tools.md).

| Package | Entry |
|---|---|
| `tddy-lsp-executor` | [session-lsp-tools](../../../packages/tddy-lsp-executor/docs/changesets/2026-10-04-session-lsp-tools.md) |
| `tddy-index-daemon` | [session-lsp-tools](../../../packages/tddy-index-daemon/docs/changesets/2026-10-04-session-lsp-tools.md) |
| `tddy-daemon` | [session-lsp-tools](../../../packages/tddy-daemon/docs/changesets/2026-10-04-session-lsp-tools.md) |

`tddy-tool-engine` and `tddy-session-lifecycle` are unchanged: the worktree every `LspExecutor` method
receives is already the host's, so the binding is that the queried file lies inside it.

**Decisions.**

- The executor lives in `tddy-lsp-executor`, not `tddy-tool-engine`: the tool engine is a dependency of
  `tddy-sandbox-runner`, which runs inside every jail, and an index client there would ship the
  restructure engine into it.
- `IndexChannel` is a port rather than `IndexDaemonRegistry`, which lives in `tddy-daemon`, a dependent
  of `tddy-lsp-executor`.
- `ReadLints` is refused rather than answered by a language server of the executor's own, which would
  disagree with the index.

**Tests.** `tddy-lsp-executor`: `tests/lsp_tools_via_index.rs` (13) and the binding's unit tests (13);
`tddy-index-daemon`: `tests/code_index_service_acceptance.rs` (39). Scoped runs of
`tddy-lsp-executor` and `tddy-index-daemon`: 168 passed, 0 failed. `code_navigation_acceptance` (7) and
`mcp_tool_advertisement_audit` (3) stay green.

**Code issues.** `oversized-file-runtime` grew 1,636 → 1,650 production lines and
`complexity-runtime-build` 891 → 905 (+14, the executor selection inside `build`); the split is deferred
with the developer's consent because other `#live-plan` nodes touch the file. Both records stay, with
the new row. Backlog entry `2026-10-03-session-lsp-tools-grew-runtime-rs` records the deferral.

**Backlog.** No entry was resolved by this change. Added:
`2026-10-04-read-lints-is-refused-through-the-warm-index` and
`2026-10-03-session-lsp-tools-grew-runtime-rs`. A jail naming another session's worktree over the host
bridge (`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge`) is
not widened by this change and not fixed by it.

**Not pinned.** `Symbols` with a `query` has no index-daemon acceptance test, because the shared fake
language server answers only `textDocument/documentSymbol`. A symlink inside a worktree that points
outside it is followed by the host's binding and the service's path check alike.
