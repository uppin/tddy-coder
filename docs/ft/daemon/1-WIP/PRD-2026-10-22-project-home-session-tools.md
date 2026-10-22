# Project-home tools — start sessions, see their status, read their worktrees - PRD

**Date**: 2026-10-22
**PRD Type**: Enhancement
**Stack**: #project-home 5/7 (`feature/project-home/home-session-tools`, base `feature/project-home/home-session`)

## Affected Features

- **Primary Feature**: [Project concept](../project-concept.md) — the project-home session's tool set.
- **Related Feature**: [Remote-codebase mode](../remote-codebase-mode.md) — new exec tools served by `ExecuteTool`.
- **Related Feature**: [Agent session status](../agent-session-status.md) — the status a listed session reports.

## Summary

The project-home agent gets four tools, scoped to **its own project**:

| Tool | Does |
|---|---|
| `ListProjectSessions` | every other session of the project: id, branch, worktree path, active or not, agent status, last activity, waiting-for-input |
| `StartProjectSession` | starts a new session in the project (prompt, branch intent, optional agent — default the project's home agent is **not** assumed; the session's own default applies), returns its id |
| `ReadSessionWorktree` | reads a file or lists a directory inside a target session's worktree |
| `DiffSessionWorktree` | the target worktree's diff against its base (uncommitted + committed-since-base), read-only |

All four are read-only towards the target's files; `StartProjectSession` creates a new session and
changes nothing existing.

## Background

The home agent is read-only on purpose; these tools are how it does useful work anyway — fan out
work to sessions, watch them, and review what they produced. Git actions on those worktrees are node 7.

## Proposed Changes

### What's Changing
- The four tools are advertised to and executable by `project-home` sessions only. Any other session
  calling them gets a refusal naming the role requirement.
- A target is a session id; it must belong to the home session's project, or the call is refused. The
  home session itself is not a valid target.
- A started session is tagged with the home session as its orchestrator, so its origin is recorded.
- `ReadSessionWorktree` refuses paths that escape the target worktree.

### What's Staying the Same
- The home session's read-only catalog (node 4) — these tools are added to it, nothing is removed.
- `spawn_conversation` / `subagent_*` for every session.

## Impact Analysis

### Technical Impact
- Tool engine (new tool defs + dispatch), worktree-service (target resolver), daemon-rpc (`ExecuteTool`
  wiring to the session listing and start path), tddy-tools (advertisement through the existing dynamic catalog).

### User Impact
- The home agent can be asked "start a session to fix X", "what are my sessions doing", "show me
  what session Y changed". No UI change.

## Implementation Plan
1. Target resolver (session id → worktree, branch, project, active).
2. The four tools in a new tool-engine module, role-gated.
3. `ExecuteTool` wiring for listing and starting.

## Acceptance Criteria
- [ ] A project-home session lists its project's other sessions with status and worktree.
- [ ] A project-home session starts a session in its project, tagged with itself as orchestrator.
- [ ] A project-home session reads a file and lists a directory in another session's worktree.
- [ ] A project-home session gets a target worktree's diff against its base.
- [ ] A target in another project, the home session itself, or a path escaping the worktree is refused.
- [ ] A non-home session calling any of the four tools is refused.

## References

### Affected Features (Complete List)
- [project-concept.md](../project-concept.md), [remote-codebase-mode.md](../remote-codebase-mode.md), [agent-session-status.md](../agent-session-status.md)

### Related Documentation
- Predecessor: `feature/project-home/home-session` (4/7). Successor: `feature/project-home/home-git-tools` (7/7) reuses the target resolver.
