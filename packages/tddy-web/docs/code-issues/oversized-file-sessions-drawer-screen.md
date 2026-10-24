# oversized-file: SessionsDrawerScreen.tsx

**Location:** `packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` step 3.5 file-length gate on PR #741 (`#start-progress` 8/8)
**Metrics:** **504 lines** (non-test source, whole file) · budget 500 · **1.008× over**
**Thresholds breached:** length 504 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 497 | base of PR #741 (`origin/feature/start-progress/progress-ui`, under budget) |
| 2026-10-24 | 504 | +7 in PR #741: the drawer's `taskClient` (from `useSessionsDrawerClients`) and the owning host's `activeTaskClient` (from `useSessionsDrawerOwningHost`), chosen by mode and passed to `SessionMainPane` the way every other per-service client is, so the create pane and the PR-Stack dialog can cancel a start. **Deferred with the developer's consent, #start-progress 8/8** |

## What would close it

The `SessionMainPane` prop block picks each per-service client the same way: the selected daemon's
while creating, the owning host's otherwise. Move that choice into `useSessionsDrawerOwningHost`
(or a `useSessionPaneClients(mode)` beside it) so it returns the one client per service the pane
receives, and spread the result. That takes about forty lines out of this file. Hand split
(TypeScript), green baseline first (`SessionsDrawerAcceptance.cy.tsx`,
`SessionsDrawerCrossHostAcceptance.cy.tsx`), mechanical move only.
