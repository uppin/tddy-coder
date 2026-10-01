# 2026-10-01 — `subagent_pull`, a per-conversation pull ledger, and `subagent_end` takes a range

**Type:** Feature

`#agent-worktree` 4/4, PR [#563](https://github.com/uppin/tddy-coder/pull/563). Cross-package entry:
[2026-10-01-agent-worktree-range-pull.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-range-pull.md).

- `subagent_pull { sessionId, from?, to? }` (`src/subagent_pull.rs`, its own module because `server.rs`
  is over the file budget) pulls through `pull_conversation_range` with the conversation's ledger and
  records the commits applied; refused while a turn is outstanding.
- `subagent_end` accepts the same `from` / `to` and pulls through the same function, then removes.
- `PullLedger` (`src/pull_ledger.rs`) holds the commits a conversation has pulled; it is dropped when
  the conversation is discarded. `annotated_turn_result` adds `droppedPulledCommits` to a turn
  outcome's `worktreeReset` from the ledger, and a turn collected twice reads the same answer.
- `server.rs` gains the route and the two annotation call sites. `mcp_tool_advertisement_audit` pins
  the tool. Pinned by `subagent_pull_mcp_acceptance` (4) and the ledger's 7 unit tests.
