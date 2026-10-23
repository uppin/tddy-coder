# 2026-10-23 — Agent activity as a right sidebar, open by default for managed sessions

**Type:** Feature

`#project-home` 3/7 (#698). `SessionWithActivitySidebar` wraps the session main pane's body: it owns the `useAcpReplay` subscription, renders the activity toggle (with its unread badge) into the pane's header through a `toolbar` render prop, and docks `SessionActivitySidebar` (`agent-activity-sidebar`, a 360px column; a full-width overlay below the `md` breakpoint) to the right of the unchanged, never-remounted body. The open state defaults to `defaultActivitySidebarOpen(session)` (`activitySidebarState.ts`: `SessionEntry.managedCodebase`); a toggle overrides it for the selected session only. Suppressed while the Activities view is the base view; counted activity is marked seen meanwhile. `hooks/useIsMobile.ts` gains the width-only `useIsNarrowViewport` / `isNarrowWidth` (touch-capable desktops keep the docked column); both hooks share one resize subscription helper. `AgentActivityOverlay` is no longer mounted (`FIXME(activity-sidebar)`, [backlog entry](../../../../docs/dev/todo/2026-10-23-agent-activity-overlay-has-no-production-mount.md)).

Decided at wrap (green's open calls): the toggle and sidebar appear only once the session has a transcript entry, as the popover's icon did — an empty open column on a fresh managed session would read as broken; and the replay hook stays subscribed while suppressed, because unmounting it would remount the session view and cancel an open agent conversation.

Code issue closed: `oversized-file-session-main-pane` — `SessionMainPane.tsx` measured **438 lines** (`wc -l`, budget 500; 443 at the base, 674 at first detection on 2026-10-04). The drop below budget came from #663's hook extraction; this PR's −5 confirmed it, and the record is deleted.

Verified: `bun test` `activitySidebarState.test.ts` + `useIsMobile.test.ts` 8/8; Cypress component `SessionActivitySidebarAcceptance` 3/3 and `InactiveSessionActivitiesAcceptance` 18/18.
