# 2026-10-01 — `diff_conversation_worktree` sends the `Diff` op

**Type:** Feature

`#agent-worktree` 3/4, PR [#562](https://github.com/uppin/tddy-coder/pull/562). Cross-package entry:
[2026-10-01-agent-worktree-diff.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-diff.md).

`diff_conversation_worktree(conversation, from, to)` in `src/conversation.rs` asks the facilitating
daemon for the diff over the same four transports as `conversation_worktree`; an omitted bound is
sent empty. It answers the `result_json` (`{"diff": {…}}`) or an `{"error", "is_error": true}` body.
Like `reset_conversation_worktree` it is a function, not a `ConversationWorktreeOp` variant, because
that enum is `Copy`. Pinned by `a_diff_names_both_bounds_and_leaves_an_omitted_one_empty`.
