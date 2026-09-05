# 2026-07-12 — Session Inspector "Usage" tab

**Type:** Feature

live per-conversation token breakdown + summing TOTAL row, streamed over `TddyRemote.Stream` (`tokenUsageUpdated`). `useSessionUsage` (owned by `SessionUsageTab` so the stream opens only while the tab is viewed) keeps the latest `ConversationRecord` snapshot; `SessionUsageTab` renders the table; `formatTokens`/`sessionUsage` helpers; `"usage"` added to `InspectorTab` + `InspectorTabs`, rendered by `SessionInspectorDrawer` with `serverIdentity` threaded from `SessionMainPane`. Feature [session-usage-inspector.md](../../../../docs/ft/web/session-usage-inspector.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#295](https://github.com/uppin/tddy-coder/pull/295). (tddy-web)
