# 2026-10-23 — Session drawer grouped per project

**Type:** Feature

`#project-home` 2/7 (#697). `groupSessionsByProject` (`src/utils/sessionProjectGroups.ts`) groups sessions under known project names, ordered by name, with a trailing **Other** group; `SessionProjectGroup` and `projectGroupCollapse.ts` render a collapsible header and persist the collapsed set in `localStorage` (unreadable storage means all expanded). `SessionDrawer` renders one group per project, each running the existing Active / Remaining partition and stack nesting over its own sessions, and expands the group of the selected session. `SessionsDrawerScreen` passes the `ListProjects` rows it already loads as `projects`.

Verified: `bun test src/utils` 150/150; Cypress component `SessionsDrawerProjectGroupsAcceptance` 4/4 and the regression specs `SessionDrawer` 7/7, `SessionsDrawerAcceptance` 23/23, `SessionsDrawerActiveSeparator` 8/8, `SessionsDrawerBulkDeleteAcceptance` 2/2, `SessionsDrawerCrossHostAcceptance` 8/8.

Recorded: `SessionDrawer.tsx` grew 453 → 534 lines; the split is deferred because #698 and #699 also modify it ([code issue](../code-issues/oversized-file-session-drawer.md), [backlog entry](../../../../docs/dev/todo/2026-10-23-session-drawer-grew-past-500-by-project-groups.md)).
