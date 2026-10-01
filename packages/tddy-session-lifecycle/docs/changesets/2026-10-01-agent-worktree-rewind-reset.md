# 2026-10-01 — `run_conversation_worktree_op` serves `Reset`

**Type:** Feature

`#agent-worktree` 2/4, PR [#561](https://github.com/uppin/tddy-coder/pull/561). Cross-package entry:
[2026-10-01-agent-worktree-rewind-reset.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-rewind-reset.md).

The `Reset` arm in `connection_service::conversation_worktree_op`, shared by the exec-tool RPC and the
jail bridge, resolves the conversation's worktree (`existing`; none answers `{"reset": null}`), maps
an empty commit to `ResetTarget::Base`, calls `reset_to` and answers `{"reset": outcome}`. Covered at
the exec-tool seam by `conversation_worktree_reset_acceptance` in `tddy-daemon-rpc` (2).
