# Stack merge, repoint and rebase run by the daemon in the node's worktree — PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#orchestrators` 10/11 (`feature/orchestrators/stack-git-tools`)

## Affected Features

- **Primary**: [PR stacking § PR-management tools](../pr-stacking.md#pr-management-tools),
  [§ Merge and repoint](../pr-stacking.md#merge-and-repoint)
- [Project concept § Git tools](../../daemon/project-concept.md#git-tools) — push remote and credential

## Summary

The remaining stack tools become daemon exec tools addressed by `stack_id`, offered to main,
orchestrators and pr-stack orchestrators: `pr_merge`, `pr_close`, `pr_repoint`, `pr_set_parents`,
`pr_resolve_conflicts`, `pr_spawn_child`. Every git step runs **on the daemon, in the node's own
worktree** (found by its branch), never in the caller's checkout:

- `pr_set_parents` / `pr_repoint` rebase the node's branch in its worktree and force-push with lease.
  A node whose worktree is dirty or whose session is running is refused naming why.
- `pr_resolve_conflicts` tries the base merge in the node's worktree, aborts it, marks the node
  `has-conflicts`, and answers the conflicted paths — plus, with `delegate: true`, sends the node's
  session a message asking it to resolve them (`SendSessionMessage`, `#orchestrators` 4/11) or resumes
  it with that message when stopped.
- `pr_spawn_child` starts the node's session through the daemon's start path with `stack_parent`
  set; the caller is recorded as its orchestrator.
- Pushes go to the project's configured remote with the project account's credential.

## Proposed Changes

### What's Changing
- Six tools in the daemon's stack-tools family; the `tddy-tools` copies and the toolcall
  `spawn-child` relay removed.
- `git_ops::rebase_onto` runs in the worktree that holds the branch.
- Daemon-run push names the remote and carries the token.

### What's Staying the Same
- Crash safety (the stack-op journal, idempotent repoint); tool names and answers.

## Acceptance Criteria
- [ ] Main merges, closes and repoints a node's PR from a read-only session
- [ ] `pr_set_parents` rebases the node in its own worktree and pushes to the project's remote
- [ ] A dirty or running node is refused without touching it
- [ ] `pr_resolve_conflicts` with `delegate` asks the node's session to resolve the named paths
- [ ] `pr_spawn_child` from main starts the node's session linked to main
- [ ] All six work on a stack owned by another host

## References
- [PR stacking](../pr-stacking.md), [Project concept](../../daemon/project-concept.md)
