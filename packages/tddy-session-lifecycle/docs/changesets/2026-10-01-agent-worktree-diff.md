# 2026-10-01 — `run_conversation_worktree_op` serves `Diff`

**Type:** Feature

`#agent-worktree` 3/4, PR [#562](https://github.com/uppin/tddy-coder/pull/562). Cross-package entry:
[2026-10-01-agent-worktree-diff.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-diff.md).

The `Diff` arm in `connection_service::conversation_worktree_op`, shared by the exec-tool RPC and
the jail bridge, resolves the conversation's worktree (`existing`; none is `FailedPrecondition`
naming the conversation), maps an empty bound to an omitted one, calls `diff` and answers
`{"diff": outcome}`. A bound the conversation lacks is `FailedPrecondition` too, through `refused`.
The arm lives here rather than in `tddy-daemon-rpc` because this function is the one both entry points
call. Covered at the exec-tool seam by `conversation_worktree_diff_acceptance` in `tddy-daemon-rpc` (2).
