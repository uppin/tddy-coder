# 2026-10-04 — Restructure tool calls in sessions, answered by the warm index

**Type:** Feature

`#live-plan` 14/15 — PR [#573](https://github.com/uppin/tddy-coder/pull/573),
`feature/live-plan/session-restructure-tools`. Product entry:
[2026-10-04-session-restructure-tools.md](../../ft/coder/changelog/2026-10-04-session-restructure-tools.md).

| Package | Entry |
|---|---|
| `tddy-lsp-executor` | [session-restructure-tools](../../../packages/tddy-lsp-executor/docs/changesets/2026-10-04-session-restructure-tools.md) |
| `tddy-toolcall` | [session-restructure-tools](../../../packages/tddy-toolcall/docs/changesets/2026-10-04-session-restructure-tools.md) |
| `tddy-tool-engine` | [session-restructure-tools](../../../packages/tddy-tool-engine/docs/changesets/2026-10-04-session-restructure-tools.md) |
| `tddy-tools` | [session-restructure-tools](../../../packages/tddy-tools/docs/changesets/2026-10-04-session-restructure-tools.md) |
| `tddy-daemon` | [session-restructure-tools](../../../packages/tddy-daemon/docs/changesets/2026-10-04-session-restructure-tools.md) |
| `tddy-session-lifecycle` | [session-restructure-tools](../../../packages/tddy-session-lifecycle/docs/changesets/2026-10-04-session-restructure-tools.md) |

**Decisions.**

- The executor lives in `tddy-lsp-executor`, beside `IndexChannel` and the worktree binding, not in
  `tddy-tool-engine`: the engine is a dependency of `tddy-sandbox-runner`, which runs in every jail, and an
  index client there would ship the restructure engine into it. The port went to `tddy-toolcall`, the
  `LspExecutor` precedent.
- No `IN_JAIL_RELAYABLE_EXEC_TOOLS` entries. That list holds `(service, method)` coordinates relayed beside
  `ExecuteTool`, and `tddy-sandbox-runner` decodes every entry as a `ConversationWorktreeRequest`; these
  tools are names carried inside `ExecuteTool`, as the `Lsp*` names are, and an entry would be wrong.
- A refusal part-way through a run is an `Ok` answer carrying `refusal`, so applied operations are never
  lost; a failed stale lookup is reported as `stale_error` beside it.

**Tests.** `tddy-lsp-executor`: `tests/restructure_tools_via_index.rs` (14), plus the existing suites;
`tddy-tools`: `mcp_tool_advertisement_audit` (4). Scoped runs of `tddy-lsp-executor`, `tddy-tool-engine` and
`tddy-toolcall`: 123 passed, 0 failed; `cargo clippy --all-targets -- -D warnings` clean on those and on
`tddy-tools`, `tddy-daemon` and `tddy-session-lifecycle`. The `tddy-daemon` and `tddy-session-lifecycle`
suites were not run: the registration and the env export are verified by compile and clippy only.

**Code issues.** Final measurements (2026-10-04): `oversized-file-runtime` 1,668 → 1,677 production lines
and `complexity-runtime-build` 928 → 937 (+9, the registration inside `build`) — split deferred, the stack's
other nodes touch the file; `oversized-file-lib` (`tddy-tool-engine`) 869 → 873 production lines (+4) —
split deferred with the developer's consent. `oversized-file-server` (`tddy-tools`) +5 lines of total file
length, and the two `tddy-session-lifecycle` sandboxed-start records +1 line each, are noted on their
records, which stay open.

**Backlog.** No entry was resolved by this change. Added:
`2026-10-04-session-restructure-tools-grew-two-oversized-files` and
`2026-10-04-session-restructure-tools-no-jail-end-to-end-test`. A jail naming another session's worktree
over the host bridge
(`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge`) is not widened
by this change: plan paths are resolved inside the host-bound worktree, and nothing from the jail names a
root.

**Not pinned.** `restructure_anchors` with an `at` range and the optional `deep`, `resume`, `from` and
`stop_after` arguments have no test of their own; the request defaults (`deep: false`, `file_budget: 0`,
`dry_run: false`) are asserted but not stated by the tool's schema.
