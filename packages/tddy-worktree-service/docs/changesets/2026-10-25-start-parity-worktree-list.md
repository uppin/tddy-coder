# 2026-10-25 — One worktree snapshot for the screen and the project-home tools

**Type:** Feature

`#start-parity` 3/9 ([#752](https://github.com/uppin/tddy-coder/pull/752)).
`worktrees::project_worktree_snapshot` / `WorktreeSnapshotRow` (diff summary + size state, no walk)
now builds `StreamWorktreeStats`' first frame, and the project-home `ListWorktrees` reads the same
function. `service::project_main_repo_on_host` is `resolve_main_repo`'s body made public;
`WorktreeServiceImpl::worktree_size_calculator()` shares the calculator; `DAEMON_SIZE_WALK_PERMITS`
names the cap of 2. `service.rs` 772 → 808 and `worktrees.rs` 975 → 1003 production lines — splits
deferred (stack-shared), recorded in `docs/code-issues/`.
