# 2026-10-04 — Session start phases and code-index warm-up progress

**Type:** Feature

`#live-plan` 12/15 — PR [#571](https://github.com/uppin/tddy-coder/pull/571),
`feature/live-plan/indexing-indicators`. Cross-package entry:
[2026-10-04-indexing-indicators.md](../../../../docs/dev/changesets/2026-10-04-indexing-indicators.md). Product entry:
[2026-10-04-indexing-indicators.md](../../../../docs/ft/web/changelog/2026-10-04-indexing-indicators.md).

`session.proto` gains `StartPhase` (`Step`: worktree, semantic index, agent; `Boundary`: begin, end) and
`StartSessionEvent.phase` (field 3). `code_navigation.proto` gains `WatchCodeIndex(session_token,
session_id)` and `CodeIndexProgress` — `code_index.IndexProgress` plus `error`, declared here because the
web reads only this package's protos. The TypeScript bindings are regenerated. Docs:
[start-session-phases.md](../start-session-phases.md), [code-navigation-proto.md](../code-navigation-proto.md).

**Decisions.** Two enums rather than a message per step: one `oneof` arm, and a consumer tracks the last
BEGIN without its END; a failed step sends no END. `error` is a field of the progress message rather than
a stream failure, so a failed warm is a normal last message.

**Code issues.** None of this package's records is affected.
