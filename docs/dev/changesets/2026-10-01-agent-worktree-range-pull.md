# 2026-10-01 — A caller pulls a chosen range of a subagent's commits, mid-conversation or at its end

**Type:** Feature

`#agent-worktree` 4/4, base `feature/agent-worktree/diff` (#562), PR
[#563](https://github.com/uppin/tddy-coder/pull/563). Feature doc:
[managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md) § `subagent_pull`.

## What was delivered

`subagent_pull { sessionId, from?, to? }` applies an inclusive range of a conversation's commits to the
caller's worktree while the conversation carries on; `subagent_end` takes the same range. Each commit
is its own 3-way apply. The conversation keeps a ledger of what it handed over, so a later pull or
`subagent_end` skips it, and a rewind that drops pulled commits names them in
`worktreeReset.droppedPulledCommits`.

- **`tddy-subagent-worktree`** — `ConversationWorktree::pull_range`, `PullRange`, `RangePullOutcome`
  (`src/range_pull.rs`); `apply_3way` widened to `pub(crate)`.
- **`tddy-service`** — `PullRangeOp pull_range = 14`; TypeScript bindings regenerated.
- **`tddy-session-lifecycle`** — the `PullRange` arm of `run_conversation_worktree_op`.
- **`tddy-session-tool-client`** — `pull_conversation_range`.
- **`tddy-tools`** — `subagent_pull`, `PullLedger`, `subagent_end`'s range, the outcome annotation.
- **`tddy-sandbox-recipes`** — `mcp__tddy-tools__subagent_pull` allowlisted.
- **`tddy-daemon-rpc`** — no production change; it carries the exec-tool acceptance suite for the arm.

## Decisions & Trade-offs

- **Both bounds inclusive** (a pull names the commits it applies), unlike `subagent_diff`'s git-style
  `from..to`; each tool's schema says which.
- **Ledger in `tddy-tools`, selection on the daemon** — the conversation lives in `tddy-tools`, the
  branch on the daemon; the request carries the ledger, so the daemon holds no per-conversation state.
- **Commit-by-commit apply** — a conflict is attributable to one commit, and a skipped commit is never
  folded into a neighbour's diff.
- **`Pull` stays** on the wire, unused by `subagent_end`, for a caller that wants the squashed diff.
- **Every `WorktreeError` from `pull_range` maps to `FailedPrecondition`**, as for `diff`.

## Deferred

- A range pull that fails part-way leaves the commits applied so far in the caller's worktree and
  records none in the ledger, so a retry applies them again. The failure is reported to the caller.
- The dropped-pulled annotation is unit-tested at the ledger; no test drives a real rewind after a real
  pull end to end.
- `tddy-tools/src/server.rs` and `tddy-session-tool-client/src/lib.rs` stay oversized; their splits wait
  until after the stack (`oversized-file-server.md`, `oversized-file-lib.md`, backlog entry
  `docs/dev/todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md`).

## Verification

Scoped, not whole-workspace: `cargo fmt` and clippy `--all-targets -D warnings` clean over
`tddy-subagent-worktree`, `tddy-session-tool-client`, `tddy-session-lifecycle`, `tddy-daemon-rpc`,
`tddy-tools` and `tddy-sandbox-recipes`; tests green for `tddy-subagent-worktree`,
`tddy-session-tool-client`, `tddy-daemon-rpc`, `tddy-sandbox-recipes` and `tddy-tools`
(`--no-fail-fast`). `tddy-session-lifecycle` fails only its macOS sandbox-jail suites
(`sandboxed_*`, mirror tests), which fail without this change. Everything else is CI's.

## Backlog entries and code issues

No `docs/dev/todo/` entry was resolved or deleted; the seven-files-over-budget entry still covers
`server.rs`. `oversized-file-server.md` (production lines 2,782 to 2,800, +18: the route and the two
`droppedPulledCommits` call sites) and `oversized-file-lib.md` (gate 609, unchanged) carry a dated row;
neither was closed.
