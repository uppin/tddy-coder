# 2026-11-16 — A target session's checkout lives under the target user's home

A session on an SSH host or Coder workspace is checked out at `$HOME/.tddy/repos/<repository>/.worktrees/<session>` on the target, cut from the branch and base ref the start requested, and removed with its clone registration when the session is deleted. A failed clone, fetch or cut reports the target's own words. See [remote-managed-worktree.md](../remote-managed-worktree.md).
