# 2026-10-01 — Files the conversation-worktree change grew past the file budget; splits deferred

**Category:** Deferred — developer consented 2026-10-01 (PR #560 `/pr-wrap`)
**Source:** `#agent-worktree` 1/4, changeset `2026-09-30-agent-worktree-isolated-edits`

The change's plan was "new logic goes into new modules; the over-budget files grow only by wiring
lines". That did not hold everywhere. Production lines, re-measured 2026-10-01 against `master`:

| File | Lines | Record |
|---|---|---|
| `packages/tddy-sandbox-runner/src/runner.rs` | 2,611 → 2,639 | `packages/tddy-sandbox-runner/docs/code-issues/oversized-file-runner.md` |
| `packages/tddy-sandbox-runner/src/host_relay.rs` | 939 → 949 (no record before) | `packages/tddy-sandbox-runner/docs/code-issues/oversized-file-host-relay.md` (new) |
| `packages/tddy-session-agents/src/session_agent_clone.rs` | 1,158 (not edited here; no record before) | `packages/tddy-session-agents/docs/code-issues/oversized-file-session-agent-clone.md` (new) |
| `packages/tddy-daemon-sandbox/src/sandbox_session.rs` | 916 → 917 | `packages/tddy-daemon-sandbox/docs/code-issues/oversized-file-sandbox-session.md` |
| `packages/tddy-session-lifecycle/src/workspace_tool_sandbox.rs` | 737 → 739 | `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-workspace-tool-sandbox.md` |
| `packages/tddy-discovery/src/roster/conversation.rs` | 522 → 526 | `packages/tddy-discovery/docs/code-issues/oversized-file-conversation.md` |

Shared with the rest of the stack (#561–#563 edit the same files), so their splits wait until the
stack lands:

| File | Lines | Record |
|---|---|---|
| `packages/tddy-session-tool-client/src/lib.rs` | 1,047 → 1,107 (+60, the `dispatch_request_via_*` split) | `packages/tddy-session-tool-client/docs/code-issues/oversized-file-lib.md` |
| `packages/tddy-discovery/src/subagent.rs` | 2,001 → 2,004 | `packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md` |
| `packages/tddy-tools/src/server.rs` | 2,754 → 2,764 | `packages/tddy-tools/docs/code-issues/oversized-file-server.md` |

**Why deferred:** each file already carries a standing deferral (see
[`2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md`](2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md)
and [`2026-09-28-conversation-rs-crossed-the-file-budget.md`](2026-09-28-conversation-rs-crossed-the-file-budget.md));
the growth is a few lines each except `lib.rs`, and an engine-driven split mid-stack would cascade
conflicts through #561–#563.

**What closing it takes:** per record, `code-restructuring`-driven extraction with a green baseline
before and after; the three stack-shared files after #563 lands. For `lib.rs`, start with the
`with_transport` extraction in
[`2026-10-01-session-tool-client-repeats-its-transport-selection-three-times.md`](2026-10-01-session-tool-client-repeats-its-transport-selection-three-times.md).
