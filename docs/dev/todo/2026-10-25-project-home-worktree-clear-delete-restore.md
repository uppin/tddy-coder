# 2026-10-25 — Project-home worktree tools stop at reading: no Clear, Delete or Restore

**Category:** Gap (low)
**Source:** `#start-parity` 3/9 (`feature/start-parity/worktree-list`, PR #752)

- The Worktrees screen can **Clear** (`CleanWorktree`), **Delete** (`RemoveWorktree`) and
  **Restore** (`RestoreSessionWorktree`) a worktree. The orchestrator's `ListWorktrees` and
  `GetWorktree` only read; it has no tool for any of the three.
- **Why kept:** all three are destructive and nobody asked for an agent to have them; the node's
  `## Boundaries` left them out on purpose.
- **What would close it:** three project-home tools over the same `tddy-worktree-service`
  functions the RPCs call (`clean_worktree_under_repo`, `remove_worktree_under_repo`, the restore
  path), refused while a session owning the worktree is active — the git tools' activity gate — and
  forwarded to another host the way the other target tools are. Acceptance cases beside
  `packages/tddy-daemon-rpc/tests/worktree_tools_acceptance.rs`.
