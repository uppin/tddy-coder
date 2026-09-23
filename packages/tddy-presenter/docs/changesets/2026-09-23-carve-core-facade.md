# 2026-09-23 — The crate is created from `tddy-core`'s presenter

**Type:** Refactor · `#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-core-facade.md`](../../../../docs/dev/changesets/2026-09-23-carve-core-facade.md)

Created from `tddy-core`'s `presenter/` (as #495 partitioned it), `post_workflow.rs`, `usage_watcher.rs` and the `cfg(test)` `test_support`, moved with `git mv`. The one call of `start_goal_for_session_continue` names it at `crate::workflow`. It depends on every crate beneath it. Four test binaries and eight `complexity-*` records (all unchanged) moved in. New code-issue record: `oversized-file-workflow-runner` (1,015 production lines, inherited). `tddy-core` re-exports it whole. See [architecture.md](../architecture.md).
