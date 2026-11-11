# 2026-11-10 — tddy-service: StreamSessionUsage; TokenUsageUpdated removed

`activity.proto` gains `StreamSessionUsage`, `StreamSessionUsageRequest`, `SessionUsageSnapshot` and
`UsageConversation`. `remote.proto`'s `TokenUsageUpdated` and `ConversationRecord` are removed, with
field 13 and `token_usage_updated` reserved; the `convert.rs` arm and its test go with them. TS
regenerated. `#usage-binding 2/5`, PR [#891](https://github.com/uppin/tddy-coder/pull/891).
