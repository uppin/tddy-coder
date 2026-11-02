# 2026-11-02 — A stopped session's tab strip

**Type:** Feature

`DormantSessionRuntime` gives a stopped session its tab strip (Agent transcript slot, daemon
terminals, agent conversations, "+"), and agent-conversation bodies move out of `SessionRuntime` into
`SessionAgentConversationLayer`, portalled into per-session host elements that the visible strip
adopts. Shared pane chrome lives in `SessionPaneChrome.tsx`. See
[inactive-session-activities.md](../inactive-session-activities.md#the-stopped-sessions-strip) and
[session-agent-conversation.md](../session-agent-conversation.md#1-conversation-bodies-are-owned-by-neither-strip).
`#session-tabs` 1/7, cross-package entry `docs/dev/changesets/2026-11-02-session-tabs-dormant-tabs.md`.
