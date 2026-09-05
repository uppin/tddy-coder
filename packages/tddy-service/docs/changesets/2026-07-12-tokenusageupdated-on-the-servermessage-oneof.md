# 2026-07-12 — `TokenUsageUpdated` on the `ServerMessage` oneof

**Type:** Feature

proto `TokenUsageUpdated { repeated ConversationRecord conversations }` + `ConversationRecord` messages and `token_usage_updated = 13` on `ServerMessage.event`; `event_to_server_message` maps `PresenterEvent::TokenUsageUpdated` field-for-field. Reuses the existing `TddyRemote.Stream` path — no new endpoint. Feature [session-usage-inspector.md](../../../../docs/ft/web/session-usage-inspector.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#295](https://github.com/uppin/tddy-coder/pull/295). (tddy-service)
