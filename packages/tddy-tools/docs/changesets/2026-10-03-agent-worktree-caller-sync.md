# 2026-10-03 — `syncWorktree` and the conversation sync port

**Type:** Feature

PR [#576](https://github.com/uppin/tddy-coder/pull/576). Cross-package entry:
[2026-10-03-agent-worktree-caller-sync.md](../../../../docs/dev/changesets/2026-10-03-agent-worktree-caller-sync.md).

- `ConversationWorktreeSyncPort` (`src/worktree_sync_port.rs`) — `ConversationWorktree { sync }` through
  `tddy_session_tool_client::sync_conversation_worktree`, mapped to `SyncAnswer` (`sync_from_answer`);
  wired in `subagent_config_for_conversation`.
- `syncWorktree` on `subagent_prompt` and `subagent_resume` (`src/sync_worktree_choice.rs`:
  `with_sync_worktree_choice`, `sync_worktree_property`, `SYNC_WORKTREE_ARG`); a non-boolean is
  refused by name.
- `src/worktree_answer.rs`: the reset and sync ports' shared answer parsing.
- Tests: `subagent_sync_worktree_mcp` (3).

`oversized-file-server`: 2,800 → 2,810 — row recorded; split deferred with consent.
