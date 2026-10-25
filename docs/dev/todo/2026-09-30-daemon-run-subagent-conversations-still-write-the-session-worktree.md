# 2026-09-30 — Peer-owned daemon-run subagent conversations still write their clone directly

**Category:** Future enhancement
**Source:** `#agent-worktree` stack, `2026-09-30-agent-worktree-isolated-edits` changeset

The conversation worktree (per-conversation branch and worktree, one commit per mutating call,
`subagent_end` / `subagent_cancel` hand-over) covers only the **in-process** subagent loop in
`tddy-tools`, whose Managed dispatch sends `ExecuteTool{conversation_id}`.

Conversations whose loop the **daemon** runs keep today's behaviour:

- `open_local` → `local_agent_codebase_access`
  (`packages/tddy-session-lifecycle/src/connection_service/svc_start_hosted_agent_clone.rs`) runs
  every call on the session worktree;
- `open_owned` → `owned_agent_codebase_access` → `run_hosted_clone_tool` proxies mutations to the
  facilitator's worktree;
- a peer's `RemoteAgentSession` inherits whichever of those the owning daemon uses.

## Why it was deferred

The developer scoped v1 to in-process agents. The daemon-run path needs its own design: the
`SessionAgentService` conversation RPCs would have to carry end/pull/diff operations, the turn
frames would carry `worktreeChange` (and then hit the chunk-framing budget recorded in
`2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md`), and the peer-owned
case collides with the roster PRD's non-goal *"Write-back from a remote clone"*.

## What it would take

`local_agent_codebase_access` wrapping its dispatch in `tddy_subagent_worktree::run_in_conversation`
keyed by the daemon-side conversation id is the cheap half. The wire half is the rest.

## Narrowed 2026-10-24 (#724, `#coagents` 5/8)

The **local** case is resolved by `clone` placement: a daemon-run conversation (`open_local`) of a
`clone`-placed agent works on its own conversation worktree, synced before every turn, and
`subagent_diff` / `_pull` / `_end` reach it through the existing `ConversationWorktree` RPC
(docs/ft/coder/managed-codebase-subagents.md § A daemon-run conversation of a `clone`-placed agent).
`shared` stays the default by the developer's choice, so a `shared` agent writing the session
worktree is intended, not a gap.

What remains is the **peer-owned** case: `open_owned` → `owned_agent_codebase_access` still proxies
mutations to the owning daemon's clone, and a peer's `RemoteAgentSession` inherits it — the roster
PRD's non-goal *"Write-back from a remote clone"*. Remote placement is `#coagents` 8/8
(`remote-codebase`).
