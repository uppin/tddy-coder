# 2026-10-01 — A caller can read the diff between any two points of a subagent conversation

**Type:** Feature

`#agent-worktree` 3/4, base `feature/agent-worktree/rewind-reset` (#561), PR
[#562](https://github.com/uppin/tddy-coder/pull/562). Successor: `range-pull`
([#563](https://github.com/uppin/tddy-coder/pull/563)). Feature doc:
[managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md) § `subagent_diff`.

## What was delivered

`subagent_diff { sessionId, from?, to? }` returns the unified diff between two commits of a
conversation's branch: `from` exclusive, `to` inclusive, defaults base and tip. The reply is
`{from, to, files, lines, diff, truncated}`; the text is capped at 64 KiB, cut at a line boundary,
while the counts cover the whole range. A commit outside the conversation, a dropped one, a `from`
that is not an ancestor of `to` and a conversation with no worktree are refused. It is read-only and
allowed while a turn runs.

- **`tddy-subagent-worktree`** — `ConversationWorktree::diff`, `ConversationDiff`,
  `DIFF_TEXT_CAP_BYTES` (`src/diff.rs`); `resolve_commit` and `short_hashes` in `reset.rs` widened to
  `pub(crate)` (visibility only).
- **`tddy-service`** — `DiffOp diff = 13` on `ConversationWorktreeRequest`; generated TypeScript
  bindings regenerated.
- **`tddy-session-lifecycle`** — the `Diff` arm of `run_conversation_worktree_op`, shared by the
  exec-tool RPC and the jail bridge; a conversation without a worktree, or a bound it lacks, is
  `FailedPrecondition`.
- **`tddy-session-tool-client`** — `diff_conversation_worktree`.
- **`tddy-tools`** — the `subagent_diff` module (`src/subagent_diff.rs`), its schema and route;
  `mcp_tool_advertisement_audit` pins the tool (counts 45/42 to 46/43).
- **`tddy-sandbox-recipes`** — `mcp__tddy-tools__subagent_diff` allowlisted beside `subagent_cancel`.
- **`tddy-daemon-rpc`** — no production change; it carries the exec-tool acceptance suite for the arm.

## Decisions & Trade-offs

- **`from` exclusive, `to` inclusive** — git's `A..B`, so a caller can hand the same two hashes to
  `git diff`.
- **Counts over the whole range even when truncated** — the counts are the summary, the text the
  detail.
- **The diff text omits `--binary`.** A binary change shows as git's `Binary files … differ` line
  rather than a payload; pinned by `a_binary_change_shows_as_binary`.
- **The daemon arm lives in `tddy-session-lifecycle`**, in `run_conversation_worktree_op`, not in
  `tddy-daemon-rpc`: that function is shared by the exec-tool RPC and the jail bridge.
- **Every `WorktreeError` from `diff` maps to `FailedPrecondition`**, including a genuine git I/O
  failure, so a caller mistake and a daemon fault are not told apart by status.

## Deferred

- `tddy-tools/src/server.rs` grew by the five-line route; the file split waits until after the stack
  (`packages/tddy-tools/docs/code-issues/oversized-file-server.md`, backlog entry
  `docs/dev/todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md`).

## Verification

Scoped, not whole-workspace: tests, `cargo fmt` and clippy `--all-targets -D warnings` clean over
`tddy-subagent-worktree`, `tddy-session-tool-client`, `tddy-session-lifecycle`, `tddy-tools`,
`tddy-daemon-rpc` and `tddy-sandbox-recipes`. The mechanics and daemon suites drive worktrees made by
#560 and are green only on top of it. Everything else is CI's.

## Backlog entries and code issues

No `docs/dev/todo/` entry was resolved by this change and none was deleted; the seven-files-over-budget
entry still covers `server.rs`. The `oversized-file-server.md` and
`tddy-session-tool-client` `oversized-file-lib.md` records carry a dated measurement row; neither was
closed.
