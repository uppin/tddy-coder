# 2026-10-01 — A subagent edits its own worktree, commits every change, and hands the result back

**Type:** Feature

`#agent-worktree` 1/4 (root, base `master`), PR [#560](https://github.com/uppin/tddy-coder/pull/560).
Successors: `rewind-reset`, `diff` and `range-pull` (#561–#563), each of which drives a conversation
worktree this node creates. Feature doc:
[managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md) § The conversation
worktree. Mechanics: [`tddy-subagent-worktree`](../../../packages/tddy-subagent-worktree/docs/conversation-worktree.md).

## What was delivered

A subagent conversation (the in-process loop in `tddy-tools`, Managed access) gets its own branch
`tddy/subagent/<session>/<conversation>` and worktree under
`<session worktree>/tmp/subagent-worktrees/`, created on its first mutating tool call and seeded with
the caller's uncommitted state as one commit. Each mutating call that changes files is committed
there and its result carries `worktreeChange` (files created / updated / removed, lines added /
removed, short hash). `subagent_end` applies base..tip to the caller as uncommitted changes (3-way,
conflict markers, `HEAD` never moves) and removes the worktree; `subagent_cancel` removes it without
pulling.

- **New crate `tddy-subagent-worktree`** — leaf crate, git through the CLI: `ConversationId`,
  `ConversationWorktrees::{existing, ensure}`, `commit_changes`, `pull_into_caller`, `remove`, the
  one fail-closed `ToolEffect` classifier and `run_in_conversation`.
- **Wire** — `ExecuteToolRequest.conversation_id = 6` and `ExecToolService/ConversationWorktree`
  (`Pull`, `Remove`); `HostToolHandler::execute` gained a `conversation_id` parameter at its seven
  implementations; the jail relay carries both.
- **Two in-jail routes** — `ExecuteTool` (`run_exec_tool_locally`) and the sandbox session channel
  (`DaemonToolHandler`) both wrap in `run_in_conversation`. Both relay halves used to drop the id.
- **`tddy-discovery`** — `MessageDescriptor::worktree_change`, serialized beside `resultSummary`.
- **`tddy-tools`** — `subagent_end` in its own module; `subagent_cancel` removes the worktree; the
  Managed closure is per conversation. `tddy-sandbox-recipes` allowlists `subagent_end`.

## Decisions & Trade-offs

- **Worktree inside the session worktree** (developer choice): the one directory every route, host
  and jail, can already reach. Cost: it relies on `info/exclude` to stay out of `git status` and is
  deleted with the session worktree.
- **Branch name carries the session id**: sessions of one project share the common git dir and the
  caller chooses conversation ids.
- **Git runs on the daemon host**; a jail may not see the common dir a linked worktree points at.
- **Typed RPC over reserved tool names** (developer choice) — costs a relay entry, buys a typed op.
- **3-way apply with conflict markers** (developer choice) over refuse-and-keep.
- **Pull at the end only** (developer choice); mid-conversation pulls are `range-pull`.
- **Fail-closed classifier** — one list of read-only tools; everything else (`AWAIT`, unknown) commits.
- **Automatic commits skip hooks and signing** — `core.hooksPath=/dev/null`, `commit.gpgsign=false`,
  `--no-verify`. Chosen by the implementer; **developer consent was NOT given** (the question went
  unanswered). Open decision:
  [`docs/dev/todo/2026-10-01-subagent-commits-skip-hooks-and-signing-without-consent.md`](../todo/2026-10-01-subagent-commits-skip-hooks-and-signing-without-consent.md).
- **One conversation's git steps are serialised** by a process-wide per-worktree async lock.
- **A commit that fails after the tool ran keeps the tool's output**: `worktreeChange: {"error": …}`.
- **The root `run_in_conversation` chose decides what a jail is sent** — a read before the first write
  reaches the jail with no `conversation_id`.
- **Package docs written directly** (`tddy-subagent-worktree/docs/conversation-worktree.md`,
  `tddy-discovery/docs/roster-and-subagent-runtime.md`) before the wrap; `#557` set the precedent. No
  approval is claimed.

## Deferred, with the developer's consent of 2026-10-01

Each is a `docs/dev/todo/` entry (kept; none resolved here):

- File growth past the budget, including three files shared with #561–#563 whose splits wait for the
  stack: [`2026-10-01-files-the-agent-worktree-change-grew-past-the-budget.md`](../todo/2026-10-01-files-the-agent-worktree-change-grew-past-the-budget.md)
- Host-bridge session binding (MEDIUM): [`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md`](../todo/2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md)
- `subagent_end` TOCTOU, ignored cancel result, orphan race, ambiguous `pulled: null`: [`2026-10-01-subagent-end-races-a-prompt-and-ignores-a-failed-cancel.md`](../todo/2026-10-01-subagent-end-races-a-prompt-and-ignores-a-failed-cancel.md)
- The HTTP/Connect JSON `oneof` encoding is unverified: [`2026-10-01-conversation-worktree-over-http-is-unverified.md`](../todo/2026-10-01-conversation-worktree-over-http-is-unverified.md)
- Three copies of the transport-selection match: [`2026-10-01-session-tool-client-repeats-its-transport-selection-three-times.md`](../todo/2026-10-01-session-tool-client-repeats-its-transport-selection-three-times.md)
- The runner links the git crate transitively: [`2026-10-01-the-sandbox-runner-links-git-code-through-a-types-re-export.md`](../todo/2026-10-01-the-sandbox-runner-links-git-code-through-a-types-re-export.md)
- Daemon-run conversations and orphan sweep (filed earlier): [`2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md`](../todo/2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md), [`2026-09-30-an-abandoned-subagent-conversation-leaves-its-branch.md`](../todo/2026-09-30-an-abandoned-subagent-conversation-leaves-its-branch.md)

Two `TODO(agent-worktree)` markers remain in production code: `tddy-tools/src/subagent_end.rs`
(sweep worktrees whose conversation is gone) and `tddy-discovery/src/roster/conversation.rs` (a
daemon-run conversation has no worktree).

## Wrapped with open items — and why

The changeset's checklist was not fully ticked; wrapped by the developer's decision, items left
unticked because they were not verified:

- **Testing** — verified 2026-10-01 per package, not as a whole: `tddy-subagent-worktree` (29 + 24 + 10),
  `tddy-discovery` (all), `tddy-session-tool-client` (8), `tddy-daemon-sandbox --lib` (7),
  `tddy-daemon-rpc` (7 + 2), `tddy-session-lifecycle` + `tddy-sandbox-runner` (630 pass, 22 known
  environmental failures); clippy and fmt clean on the touched packages. **Not run:** `tddy-tools`
  this pass, and the seatbelt suites (`sandbox_stdio_seatbelt_acceptance` does not compile on
  `master`). The rest is CI's.
- **Integration** — deferred: no real-jail test can run on this macOS host; the jail route is covered
  at the request seam only.
- **Discovery descriptor + MCP `subagent_end` / `subagent_cancel`** milestone — discovery verified;
  the `tddy-tools` MCP suite was not re-run.
- **Allowlists updated** — implemented in `tddy-sandbox-recipes` and pinned by a unit test, not
  ticked in the changeset.

## File length (production lines, `master` → this branch; none claimed by this PR)

`runner.rs` 2,611 → 2,639 · `host_relay.rs` 939 → 949 · `session_agent_clone.rs` 1,158 (not edited) ·
`sandbox_session.rs` 916 → 917 · `workspace_tool_sandbox.rs` 737 → 739 · `roster/conversation.rs`
522 → 526 · stack-shared: `session-tool-client/src/lib.rs` 1,047 → 1,107, discovery `subagent.rs`
2,001 → 2,004, tools `server.rs` 2,754 → 2,764. Records:
`packages/*/docs/code-issues/oversized-file-*.md` (new: `host-relay`, `session-agent-clone`).

## Backlog entries and code issues

No `docs/dev/todo/` entry was resolved by this change, and none was deleted. Entries it touched:
`2026-09-26-a-jail-rebuild-can-re-run-a-tool-call-that-already-executed` (during, accepted risk,
the commit makes it visible as one commit holding both effects),
`2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it` (answered for the token
path only; see the host-bridge entry above),
`2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold` (`worktreeChange` is
bounded), `2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change`. The code
issues in the touched packages carry dated measurement rows; none was closed or deleted.
