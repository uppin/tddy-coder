# 2026-10-24 — Split start streams, no deadline

**Type:** Feature

`#start-progress` 4/8 ([#737](https://github.com/uppin/tddy-coder/pull/737)). Tests only: the two split-forward deadline tests in `remote_managed_worktree_acceptance.rs` are replaced by `a_split_start_outlives_the_old_budget_while_the_codebase_host_keeps_it_alive`; `split_forward_deadline` delegate removed from `handler_state.rs`. See [module layout](../module-layout.md). Cross-package entry: [docs/dev/changesets/2026-10-24-start-progress-split-stream.md](../../../../docs/dev/changesets/2026-10-24-start-progress-split-stream.md).
