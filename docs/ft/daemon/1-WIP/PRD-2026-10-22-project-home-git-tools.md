# Project-home git tools — commit, push, and gated destructive actions on a session's worktree - PRD

**Date**: 2026-10-22
**PRD Type**: Enhancement
**Stack**: #project-home 7/7 (`feature/project-home/home-git-tools`, base `feature/project-home/home-auto-stop`)

## Affected Features

- **Primary Feature**: [Project concept](../project-concept.md) — the project-home session's git tools.
- **Related Feature**: [Host worktree services](../host-worktree-services.md) — git operations on a session's worktree.
- **Related Feature**: [Session branch conflict](../session-branch-conflict.md) — the "active claimant" rule the gate reuses.

## Summary

The project-home agent can run git on **another session's worktree** in its project:

| Tool | Gate |
|---|---|
| `CommitSessionWorktree(target, message, paths?)` | always allowed |
| `PushSessionBranch(target)` — plain push of the session's branch | always allowed |
| `RebaseSessionBranch(target, onto)` | refused while the target session is **active** |
| `StashSessionWorktree(target)` | refused while active |
| `ResetSessionWorktree(target, mode, ref)` | refused while active |
| `CheckoutSessionWorktree(target, ref)` | refused while active |
| `CleanSessionWorktree(target)` | refused while active |
| `PushSessionBranch(target, force_with_lease: true)` | refused while active |

A refusal names the active session so the agent (or user) can stop it first.

## Background

Read-only towards files, the home agent still needs to land work: commit what a session produced,
push it, bring it onto a moved base. Anything that rewrites a worktree's files or history under a
running agent would corrupt that agent's view, so those wait until the target is stopped. Rebase
is gated with them (developer's decision, 2026-10-22).

## Proposed Changes

### What's Changing
- The tools above, role-gated to `project-home` sessions, targets resolved like node 5's tools
  (same project, not the home itself).
- A rebase that conflicts is **aborted**, leaving the worktree as it was, and the conflicting paths are returned.
- Push uses the identity the target session itself pushes with (project account / local gh).

### What's Staying the Same
- Node 5's read-only tools; the home worktree is never a target.

## Impact Analysis

### Technical Impact
- Worktree-service git operations module; tool-engine tool defs; daemon wiring through node 5's port.

### User Impact
- "Commit and push what session X did", "rebase X on main once it's stopped" from the home agent.

## Implementation Plan
1. Git operations on a resolved target (new module).
2. The activity gate.
3. Tool defs + dispatch.

## Acceptance Criteria
- [ ] Commit and plain push work on an active or inactive target.
- [ ] Rebase, stash, reset, checkout, clean and force-with-lease push work on an inactive target and are refused on an active one, naming it.
- [ ] A conflicting rebase is aborted, the worktree unchanged, the conflicting paths reported.
- [ ] Targets outside the project, or the home worktree, are refused; non-home sessions cannot call the tools.

## References

### Affected Features (Complete List)
- [project-concept.md](../project-concept.md), [host-worktree-services.md](../host-worktree-services.md), [session-branch-conflict.md](../session-branch-conflict.md)

### Related Documentation
- Predecessor: `feature/project-home/home-session-tools` (5/7) owns the target resolver.
