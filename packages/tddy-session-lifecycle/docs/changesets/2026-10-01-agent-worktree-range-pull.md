# 2026-10-01 — `run_conversation_worktree_op` serves `PullRange`

**Type:** Feature

`#agent-worktree` 4/4, PR [#563](https://github.com/uppin/tddy-coder/pull/563). Cross-package entry:
[2026-10-01-agent-worktree-range-pull.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-range-pull.md).

The `PullRange` arm in `connection_service::conversation_worktree_op`, shared by the exec-tool RPC
and the jail bridge: no worktree answers `{"pulled": null}`; otherwise it maps empty bounds to omitted
ones, hands `already_pulled` to `pull_range` as a set and answers `{"pulled": outcome}`. A bound the
conversation lacks, or a `from` after `to`, is `FailedPrecondition` through `refused`.
