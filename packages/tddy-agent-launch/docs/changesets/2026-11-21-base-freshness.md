# 2026-11-21 — Base freshness and the cut base

**Type:** Feature

- `tddy_worktree_service::base_freshness`: `base_freshness` / `refresh_home` check and fast-forward an
  orchestrator's home worktree against its project's trunk; `CutBase` and `branch_of_worktree` read what a
  session's worktree was cut from.
- `tddy_tool_engine::base_freshness_tools`: the `CheckBaseFreshness` / `RefreshHome` tools and the
  `BaseFreshnessPort` the daemon implements; `StartLedgerRecord` records `base_commit` and `base_behind`.
- `tddy_agent_launch::cli_session_worktree` reports `worktree_ready { branch, worktree, base }` for a local
  cut as well as a cut on a target.

See [project-concept.md § Keeping the base fresh](../../../../docs/ft/daemon/project-concept.md#keeping-the-base-fresh). Full record: [`docs/dev/changesets/2026-11-21-base-freshness.md`](../../../../docs/dev/changesets/2026-11-21-base-freshness.md).
