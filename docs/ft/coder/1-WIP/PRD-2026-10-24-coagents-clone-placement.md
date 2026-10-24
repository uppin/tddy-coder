# A subagent can work on its own clone instead of the main agent's worktree - PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement
**Stack**: `#coagents` 5/8, branch `feature/coagents/clone-placement`, base `feature/coagents/conversation-bubble` (#723)

## Affected Features

- **Primary Feature**: [Managed-codebase subagents](../managed-codebase-subagents.md) § The conversation worktree — extended to conversations the daemon runs
- **Related Feature**: [Session agent roster](../../daemon/session-agent-roster.md) — attach takes a placement

## Summary

A subagent attached to a session works on the **main agent's worktree** by default — a teammate in
the same checkout. This PRD adds the other choice at attach time: **`clone`**. A conversation of a
`clone`-placed agent works in its own conversation worktree — a branch and checkout of its own,
created from the main agent's current files and brought up to date with them before every turn — and
hands its work back the way in-jail subagents already do: `subagent_diff` to read it,
`subagent_pull` to take part of it, `subagent_end` to take the rest.

## Background

The conversation worktree exists today only for subagent loops that run inside the main agent's
jail; conversations the daemon runs — every coding agent, and daemon-run `model` agents — write the
session worktree directly
([`docs/dev/todo/2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md`](../../../dev/todo/2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md)).
For a coding agent that edits many files and runs tests, isolation is the safer choice when the main
agent keeps working at the same time.

## Proposed Changes

### What's Changing

- **Attach takes a placement.** `shared` (default — the session worktree) or `clone`. The roster
  entry shows it; the Add-agent picker and the create-session picker offer it per agent.
- **A `clone` conversation runs in its conversation worktree.** It is created when the conversation
  opens, synced with the main agent's current files (`HEAD` + uncommitted) before every turn, as for
  in-jail subagents.
- **Coding agents in a clone.** The agent's working directory is the conversation worktree; in a
  managed session its `tddy-tools` exec tools run there instead. Whatever the agent changed during a
  turn is committed on the conversation branch when the turn ends, so the work is listable.
- **Daemon-run `model` agents in a clone** get the per-call commits in-jail ones already get.
- **`subagent_diff` / `subagent_pull` / `subagent_end` work for daemon-run conversations** in a
  clone: they read, apply and end the conversation's branch exactly as for in-jail ones.
- **A `shared` conversation** still answers `pulled: null` (nothing to hand back — it wrote the
  worktree directly), and the tool descriptions say so.

### What's Staying the Same

- The default placement and everything a `shared` subagent does.
- In-jail subagents (always their conversation worktree, as today).
- Remote agents keep their own clone per remote daemon.

## Impact Analysis

### Technical Impact

- Proto: `AttachSessionAgentRequest.placement`, `SessionAgentEntry.placement` (enum `AgentPlacement`).
- `tddy-session-agents`: open path creates the conversation worktree for `clone` placement; exec
  tools carry the conversation id; turn-end commit for coding agents.
- `tddy-tools`: `subagent_diff` / `_pull` / `_end` call the daemon's existing `ConversationWorktree`
  operations for daemon-run conversations.
- `tddy-web`: placement choice in the two pickers and on the roster row.

### User Impact

- A developer can let a coding subagent work in parallel with the main agent without them editing the
  same files under each other.

## Acceptance Criteria

- [ ] Attaching with `clone` records the placement on the roster entry.
- [ ] A `clone` coding-agent conversation runs in its own worktree; the session worktree is untouched by it.
- [ ] Each turn starts on the main agent's current files.
- [ ] A turn's changes are one commit on the conversation branch.
- [ ] `subagent_diff`, `subagent_pull` and `subagent_end` read and apply that work into the main agent's worktree.
- [ ] A daemon-run `model` agent in a clone gets one commit per mutating call.
- [ ] A `shared` conversation still answers `pulled: null`, and the description says why.
- [ ] Both pickers and the roster row show the placement.

## References

- [Managed-codebase subagents](../managed-codebase-subagents.md) § The conversation worktree
- Stack parents: #720–#723
