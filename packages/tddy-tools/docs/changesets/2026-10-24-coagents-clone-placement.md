# 2026-10-24 — diff, pull and end reach a daemon-run clone conversation
**Type:** Feature

`worktree_route.rs` decides whether a conversation has a worktree of its own; `subagent_diff` / `_pull` / `_end` route a daemon-run `clone` conversation to the daemon's `ConversationWorktree` ops, and exec calls name `TDDY_CONVERSATION_ID` (#724, `#coagents` 5/8). See [managed-codebase subagents](../../../../docs/ft/coder/managed-codebase-subagents.md#a-daemon-run-conversation-of-a-clone-placed-agent).
