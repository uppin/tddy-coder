# 2026-11-10 — tddy-session-web: Usage tab on the daemon's stream, new columns and tree

`useSessionUsage(client, sessionToken, sessionId)` reads `ActivityService.StreamSessionUsage` on the
session host's client (no room, no `serverIdentity`); `SessionUsageTab` shows Cache read, Cache
write, Reasoning and Cost, nests rows by `buildUsageTree`, and marks a partial TOTAL cost
(`usageTotals().costPartial`). `#usage-binding 2/5`, PR [#891](https://github.com/uppin/tddy-coder/pull/891). See
[session-usage-inspector.md](../../../../docs/ft/web/session-usage-inspector.md).
