# 2026-11-14 — `rebase_onto` rebases in the worktree that holds the branch

**Type:** Feature

`git_ops::rebase_onto` (signature unchanged) finds the worktree of `repo_root` that holds
`branch` and rebases there, so a node's branch checked out in its session's worktree is rebased
in place and `repo_root`'s own checkout is never switched. A branch with no worktree, a
worktree holding another branch, and a worktree with uncommitted changes are each refused
naming why, before anything is rebased; a conflicting rebase is aborted.
