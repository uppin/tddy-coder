# 2026-11-15 — A session target's worktree is optional, and a record names its codebase half

**Type:** Fix

`SessionTarget.worktree` is an `Option<PathBuf>`; `SessionTarget::worktree()` gives
the checkout a worktree or git tool acts on, refused as `NoWorktree` for a target
that has none. `resolve_session_target` no longer returns `NoWorktree`: a
worktree-less session (a split session's agent half) resolves as a target.
`TargetRecord` and `SessionTarget` carry `codebase: Option<CodebaseHalf>` — the
daemon and workspace session holding a split agent's checkout. `run_git_op` asks for
the worktree through the accessor and reports `GitOpError::NoWorktree`.

Cross-package record: [docs/dev/changesets/2026-11-15-split-session-targets.md](../../../../docs/dev/changesets/2026-11-15-split-session-targets.md).
