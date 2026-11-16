# 2026-11-16 — Home and orchestrator sessions show no child-conversation tabs

**Type:** Fix

`useChildSessions` returns no children when the parent entry carries `session_role` `project-home`
or `orchestrator`, so those sessions show only the Agent tab: every session they started through
`StartSession` records them as orchestrator, and those are started sessions, not the spawned
conversations the tabs were designed for. An ordinary session that spawned a child keeps its tab.
Contract: [spawn-conversation.md](../../../../docs/ft/coder/spawn-conversation.md).

Tests: `SessionHomeHidesChildTabs.cy.tsx` (home hides, orchestrator hides, ordinary keeps) — 6/6
with the `SessionChildTabsAcceptance` guard; `bun test src` 388/388. PR [#928](https://github.com/uppin/tddy-coder/pull/928).
