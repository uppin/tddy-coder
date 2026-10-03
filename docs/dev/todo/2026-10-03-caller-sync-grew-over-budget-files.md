# 2026-10-03 — Files the caller-sync change grew past the file budget; splits deferred

**Category:** Deferred — developer consented 2026-10-03
**Source:** changeset [`2026-10-03-agent-worktree-caller-sync`](../changesets/2026-10-03-agent-worktree-caller-sync.md)
(`/analyze-clean-code` first pass, file-length gate)

The plan was "new logic goes into new modules; the over-budget files grow by wiring lines only".
After the refactor that followed validation — the turn phase moved out of `take_turn` into
`subagent/turn_start.rs`, the sync step and its texts into `subagent/worktree_sync.rs`, the daemon
answer parsing into `tddy-tools/src/worktree_answer.rs` — `subagent.rs` ends **below** its base. Four
over-budget files still end above it.

Production lines (to the first column-0 `#[cfg(test)]`; the whole file when it has no test module),
measured 2026-10-03 after the refactor against the branch base `0ce696aa`:

| File | Lines | What stayed | Record |
|---|---|---|---|
| `packages/tddy-tools/src/server.rs` | 2,800 → 2,810 (+10) | the two `with_sync_worktree_choice` parser calls in `subagent_prompt` / `subagent_resume` and the two `syncWorktree` schema entries; the parser, the property and the `SYNC_WORKTREE_ARG` name live in `sync_worktree_choice.rs` | [`oversized-file-server.md`](../../../packages/tddy-tools/docs/code-issues/oversized-file-server.md) |
| `packages/tddy-discovery/src/subagent_runtime.rs` | 732 → 735 (+3) | `prompt_outcome_json` adds the `worktreeSync` key | [`oversized-file-subagent-runtime.md`](../../../packages/tddy-discovery/docs/code-issues/oversized-file-subagent-runtime.md) |
| `packages/tddy-session-tool-client/src/lib.rs` | 1,110 → 1,111 (+1) | the `sync_conversation_worktree` re-export (rustfmt wrapped the `use` list onto one more line) | [`oversized-file-lib.md`](../../../packages/tddy-session-tool-client/docs/code-issues/oversized-file-lib.md) |
| `packages/tddy-session-lifecycle/src/connection_service.rs` | 511 → 518 (+7; no test module, whole file) | `DaemonRpcHandler` gains its `bound` session and the doc of what binding means; `sandbox_rpc_bridge` now holds the host's `Weak` handle. `BoundJailSession` itself lives in `connection_service/daemon_rpc_handler.rs` | none — the file crossed 500 before this change without a record; the next `/analyze-code-issues` run should write `oversized-file-connection-service.md` |

For the record, the file that shrank:

| File | Lines | Record |
|---|---|---|
| `packages/tddy-discovery/src/subagent.rs` | 2,050 → 2,046 (−4) | [`oversized-file-subagent.md`](../../../packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md) |

**Why deferred:** every residual line is a call site or a field that must sit where the dispatch,
the outcome JSON, the public re-export or the handler struct is. Moving them means splitting the
host file itself, which each record already plans (`server.rs`'s tool routes, `subagent_runtime.rs`'s
outcome-JSON rendering, `lib.rs`'s seam B, `connection_service.rs`'s handler types) — a
restructure of its own, not part of a feature PR.

**What closing it takes:** per record, a `code-restructuring`-driven extraction with a green
baseline before and after.
