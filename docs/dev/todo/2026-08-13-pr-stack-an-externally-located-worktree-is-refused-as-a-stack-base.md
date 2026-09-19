# 2026-08-13 — pr-stack — an externally-located worktree is refused as a stack base

**Category:** Future enhancement
**Source:** pr-stack-base-session changeset, 2026-08-13

`session_repo_is_in_project` accepts a base session whose canonical `Changeset.repo_path` is at or under
the project's `main_repo_path`. That covers every worktree this system creates —
`tddy_git::worktree_dir` is always `<repo_root>/.worktrees` — but git allows a worktree anywhere, and
the resolver (`tddy_git::path_is_registered_worktree_of_repo`) explicitly tolerates registered
worktrees outside `.worktrees/`. Such a session is refused as a stack base even though its branch
belongs to the right repository.

> Re-aimed by `#carve` 6/11 (PR #492): this plumbing moved from `tddy-core/src/worktree.rs` to the
> new `tddy-git` crate, byte-identical. The refusal described here is unchanged — `tddy_core::worktree`
> re-exports the crate, so the old paths still resolve.

Conservative in the safe direction (a false refusal, not a false accept) and consistent with treating
"could not tell" as "not the same repository". The real fix is to compare the resolved **git common
dir** rather than a path prefix.
