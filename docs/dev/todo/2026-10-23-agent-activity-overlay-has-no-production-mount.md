# AgentActivityOverlay has no production mount since the activity sidebar

**Raised:** 2026-10-23 by `/pr-wrap` on #698 (`#project-home` 3/7)
**Why deferred:** `SessionMainPane` now mounts `SessionWithActivitySidebar`, so `packages/tddy-web/src/components/sessions/AgentActivityOverlay.tsx` (marked `FIXME(activity-sidebar)`) is reached only by eight component specs that mount it standalone: the seven `cypress/component/AgentActivity*.cy.tsx` specs and `ActivitiesTailScrollAcceptance.cy.tsx`. Removing it means deleting or rewriting those specs, and deleting files needs the developer's go-ahead, so #698 kept it.
**What to do — the developer's choice:**
- **Delete** `AgentActivityOverlay.tsx` together with the eight specs, after checking each spec's assertions (ACP replay paging, client identity, detail dialog and its lazy fetch, lazy count persistence, snapshot in flight, tail scroll) are covered by the sidebar's specs — or move the uncovered ones first; or
- **Re-mount** the eight specs on `SessionWithActivitySidebar` (`agentActivityPage.open()` already opens the transcript through the toggle, so most bodies should carry over), then delete the overlay.

Either way the transcript behaviour stays pinned; only the mount changes. Recommendation: re-mount, since the sidebar is now the only production path those specs should protect.
