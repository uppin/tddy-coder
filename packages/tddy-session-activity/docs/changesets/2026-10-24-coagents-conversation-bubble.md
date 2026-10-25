# 2026-10-24 — Conversation-addressed replay
**Type:** Feature

The three replay RPCs honour `conversation_id`: the conversation's transcript, with a live tail over `ConversationFrames` (`service/conversation_replay.rs`); a conversation that recorded nothing is `NOT_FOUND`. See [agent activity pane](../../../../docs/ft/web/agent-activity-pane.md#a-subagent-conversation-is-one-bubble-added-2026-10-24) (#723, `#coagents` 4/8).
