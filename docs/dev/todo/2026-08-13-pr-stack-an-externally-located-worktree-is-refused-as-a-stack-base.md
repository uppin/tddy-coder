# 2026-08-13 — pr-stack — an externally-located worktree is refused as a stack base

**Category:** Future enhancement
**Source:** pr-stack-base-session changeset, 2026-08-13

`session_repo_is_in_project` accepts a base session whose canonical `Changeset.repo_path` is at or under
the project's `main_repo_path`. That covers every worktree this system creates —
`worktree::worktrees_dir` is always `<repo_root>/.worktrees` — but git allows a worktree anywhere, and
`worktree.rs`'s resolver explicitly tolerates registered worktrees outside `.worktrees/`. Such a session
is refused as a stack base even though its branch belongs to the right repository.

Conservative in the safe direction (a false refusal, not a false accept) and consistent with treating
"could not tell" as "not the same repository". The real fix is to compare the resolved **git common
dir** rather than a path prefix.
