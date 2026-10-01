# 2026-10-01 — `pull_conversation_range` sends the `PullRange` op

**Type:** Feature

`#agent-worktree` 4/4, PR [#563](https://github.com/uppin/tddy-coder/pull/563). Cross-package entry:
[2026-10-01-agent-worktree-range-pull.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-range-pull.md).

`pull_conversation_range(conversation, from, to, already_pulled)` in `src/conversation.rs` asks the
facilitating daemon for the range over the same four transports as `conversation_worktree`; omitted
bounds are sent empty and the ledger travels in `already_pulled`. Pinned by
`a_range_pull_carries_the_callers_ledger`.
