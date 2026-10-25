# 2026-10-25 — Project-home worktree listing

**Type:** Feature

`#start-parity` 3/9 ([#752](https://github.com/uppin/tddy-coder/pull/752)). `DaemonProjectHome`
implements `ProjectHomePort::list_worktrees`: it resolves the project's main repo on this host
(`project_main_repo_on_host`) and reads `worktrees::project_worktree_snapshot` off the runtime, from
the host's shared `WorktreeSizeCalculator` (`ExecToolRpcHandler.worktree_sizes`). Another host is
reached through `#start-parity` 1/9's forward.

Tests: `tests/worktree_tools_acceptance.rs` (two-host world from `tests/support/mod.rs`).
