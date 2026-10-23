# 2026-10-23 — Pinned main entry per project group
**Type:** Feature

`ProjectHomeEntry`, `useEnsureProjectHome`, `useSessionsDrawerProjectHomes`; `utils/projectHomeGroups.ts` (`projectHomeSlot`, `withProjectHomeGroups` — a group for a project with a home agent and no session, without changing `groupSessionsByProject`). See [session drawer § Pinned main entry](../../../../docs/ft/web/session-drawer.md#pinned-main-entry) (#699, `#project-home` 4/7).

Code issues: `oversized-file-session-drawer` regressed 534 → 567 (split deferred past the stack); `oversized-file-sessions-drawer-screen` opened at 511 (497 at base). Verified: `SessionsDrawerProjectHomeAcceptance` 7/7, `SessionsDrawerProjectGroupsAcceptance` 4/4, `SessionDrawer` 7/7, `SessionsDrawerAcceptance` 23/23, `projectHomeGroups.test.ts` 5/5.
