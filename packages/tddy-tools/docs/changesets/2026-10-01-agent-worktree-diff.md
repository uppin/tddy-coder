# 2026-10-01 — `subagent_diff` reads what a conversation changed

**Type:** Feature

`#agent-worktree` 3/4, PR [#562](https://github.com/uppin/tddy-coder/pull/562). Cross-package entry:
[2026-10-01-agent-worktree-diff.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-diff.md).

`subagent_diff { sessionId, from?, to? }` (`src/subagent_diff.rs`, its own module because `server.rs`
is over the file budget; `server.rs` gains only the route) sends `ConversationWorktree { diff }`
through `diff_conversation_worktree` and answers the daemon's `diff` object. It takes no
conversation lock, so it answers while a turn runs; an unknown conversation and one that never wrote
are refused by name. `mcp_tool_advertisement_audit` pins the tool (counts 45/42 to 46/43). Pinned by
`subagent_diff_mcp_acceptance` (3) over the real stdio wire.
