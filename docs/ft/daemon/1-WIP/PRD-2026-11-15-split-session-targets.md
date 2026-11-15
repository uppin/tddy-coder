# The project-home tools can inspect and drive a split session - PRD

**Date**: 2026-11-15
**PRD Type**: Bug Fix
**Stack**: `#split-alive` 3/4 — branch `feature/split-alive/split-targets`

## Affected Features

- [Project concept § Project-home tools](../project-concept.md#project-home-tools) and § Git tools
- [Remote managed worktree](../remote-managed-worktree.md) — the agent half / codebase half pairing

## Summary

Every project-home tool that targets another session refuses a split session's agent half with
"session … has no worktree" — including the tools that never touch a worktree (terminal, activity,
transcript, messages, answers, terminate, delete, marks). And the worktree tools, named with the agent
half's id, have no way to know the worktree lives on another host under another id. After this
change, agent-side tools act on the agent half where it runs, and worktree and git tools named with a
split agent's id act on its codebase half, on the codebase host, without the caller having to know the
pairing.

## Background

[Findings](../../../dev/todo/2026-11-15-split-session-agent-not-running.md) § C. `resolve_session_target`
refuses any record without a worktree before the tool runs. The orchestrator that started the
2026-11-15 smoke test could read neither the agent's terminal nor its activity, and had to discover the
codebase half's id from `ListProjectSessions` and name its host to diff it.

## Proposed Changes

### What's Changing

1. **Agent-side tools accept a session without a worktree.** `ReadSessionTerminal`, `ClaimSessionTerminal`,
   `SendSessionKeys`, `ReadSessionActivity`, `ReadSessionTranscript`, `SendSessionMessage`,
   `ResumeSession`, `AnswerSessionPermission`, `AnswerSessionQuestion`, `TerminateSession`,
   `DeleteSession`, `SetSessionMark` resolve a worktree-less session and act on it.
2. **Worktree tools follow a split agent to its codebase.** `ReadSessionWorktree`, `DiffSessionWorktree`,
   `RestoreWorktree`'s worktree checks and the git tools (`CommitSessionWorktree`, `PushSessionBranch`,
   `RebaseSessionBranch`, `StashSessionWorktree`, `ResetSessionWorktree`, `CheckoutSessionWorktree`,
   `CleanSessionWorktree`), named with a split agent half's id, run against its codebase half on the
   codebase host — as if called with that host and id. The answer names the session it acted on.
3. **A session with neither a worktree nor a codebase half** is still refused by the worktree tools,
   with the same "has no worktree" message.

### What's Staying the Same

- Naming a host explicitly (`daemon_instance_id`) and the one-hop forwarding rule.
- The project, home and orchestrator admission checks.
- Tools aimed at sessions with a worktree.

## Impact Analysis

### Technical Impact

- `tddy-worktree-service`: a target's worktree is optional; a record names its codebase half.
- `tddy-tool-engine`: worktree tools ask for the worktree explicitly; which tools are worktree tools
  is declared in one list.
- `tddy-daemon-rpc`: the project-home forward redirects a worktree tool aimed at a split agent half to
  its codebase half before resolving.

### User Impact

- An orchestrator can watch, answer, message and delete a split session by its id, and diff or commit
  its work by the same id.

## Success Criteria

- [ ] Agent-side tools on a split agent half succeed on the agent host.
- [ ] `DiffSessionWorktree` / `ReadSessionWorktree` / the git tools on a split agent half's id act on
      the codebase half on the codebase host, and report the codebase session id.
- [ ] Worktree tools on a session with no worktree and no codebase half are refused as before.

## Out of Scope

- The start result's branch/worktree and the drawer (`#split-alive` 4/4).
- Agent liveness and hooks (1/4, 2/4).
- Routing an agent-side tool named with the **codebase** half's id back to the agent half.
