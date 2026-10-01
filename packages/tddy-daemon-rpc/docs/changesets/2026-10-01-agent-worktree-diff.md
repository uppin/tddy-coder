# 2026-10-01 — The exec-tool seam carries the `Diff` acceptance suite

**Type:** Feature

`#agent-worktree` 3/4, PR [#562](https://github.com/uppin/tddy-coder/pull/562). Cross-package entry:
[2026-10-01-agent-worktree-diff.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-diff.md).

No production change: the `Diff` arm is `run_conversation_worktree_op` in `tddy-session-lifecycle`,
which this crate's exec-tool RPC calls after authorizing the token. `conversation_worktree_diff_acceptance`
(2) pins the answer for a conversation with a worktree and `FailedPrecondition` for one without.
