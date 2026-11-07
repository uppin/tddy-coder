# 2026-11-07 — A timed-out operator start cannot undo its peer-held parts

**Category:** Deferred from `#agent-tools` 23/28 (`feature/agent-tools/bounded-waits`, [#831](https://github.com/uppin/tddy-coder/pull/831)) — **planned as a follow-up stack node** (user, 2026-11-07)
**Source:** `TODO(operator-start-undo)` at `packages/tddy-agent-launch/src/start_task.rs` (`undo_a_timed_out_start`)

## What is there

A start that overruns `session_start_timeout_secs` and comes back with a session inside the grace is
undone by `undo_a_timed_out_start`, keyed by OS user: the session (`delete_session_as`) and the branch
it cut (`branch_a_start_cut_as`) are removed without a token.

The split codebase half and the agent clones are deleted on the peers that hold them —
`SplitSessions::delete_paired_codebase_session` (`packages/tddy-session-split/src/svc_paired_codebase_teardown.rs`)
and `AgentRoster::tear_down_every_agent_clone` (`packages/tddy-session-agents/src/svc_provision_agent_clone.rs`) —
and a peer authenticates that delete by the start's own session token. An operator (Telegram) start
has none, so for an operator start that created either, the delete is refused, the session is left in
place, and the refusal is logged. The caller still gets `DEADLINE_EXCEEDED`.

## What would close it

- An OS-user-keyed delete on the split host port (`tddy-session-split`) and in the agent roster
  (`tddy-session-agents`), and a peer delete protocol that authenticates the daemon rather than a
  caller's token.
- `undo_a_timed_out_start` then tears down an operator start's peer-held parts; remove the
  `TODO(operator-start-undo)`.
- Test: an operator start with a split codebase (or an agent clone) whose step never returns leaves
  no session, no paired half and no clone.
