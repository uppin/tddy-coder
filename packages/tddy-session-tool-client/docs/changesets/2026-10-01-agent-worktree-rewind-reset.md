# 2026-10-01 — `reset_conversation_worktree` asks the daemon to reset a conversation worktree

**Type:** Feature

`#agent-worktree` 2/4, PR [#561](https://github.com/uppin/tddy-coder/pull/561). Cross-package entry:
[2026-10-01-agent-worktree-rewind-reset.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-rewind-reset.md).

`reset_conversation_worktree(conversation_id, commit: Option<&str>)` in `src/conversation.rs` sends the
`Reset` op over the same transports as `conversation_worktree`; `None` sends an empty commit, which
names the base. It is a function rather than a variant of the `Copy` enum `ConversationWorktreeOp`
because a reset carries data. The transport-selection match is shared through
`ask_conversation_worktree`. Pinned by `a_reset_to_a_commit_names_the_commit` and
`a_reset_to_the_base_sends_an_empty_commit`. `src/lib.rs` is over the file budget; its split is
deferred until after the stack (`docs/code-issues/oversized-file-lib.md`).
