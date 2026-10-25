# 2026-10-24 — A coding session exposes its update stream
**Type:** Feature

`CodingAgentSession` broadcasts every streamed `AcpUpdate` (`SubagentSession::updates`) and accepts a recorder (`record_updates_with`); both share the turn's one fan-out. `session.rs` 501 → 530, recorded, not split (stack-shared). See [agent activity pane](../../../../docs/ft/web/agent-activity-pane.md#a-subagent-conversation-is-one-bubble-added-2026-10-24) (#723, `#coagents` 4/8).
