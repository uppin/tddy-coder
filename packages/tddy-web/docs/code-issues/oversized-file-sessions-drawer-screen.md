# oversized-file: SessionsDrawerScreen.tsx

**Location:** `packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx`
**Category:** oversized-file
**Detected:** 2026-10-23 by `/pr-wrap` on PR #707 (`#visible-failures` 2/2)
**Metrics:** **500 lines** (whole file, TS) · budget 500 · **1.00×, at the budget with no headroom** — `wc -l packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx`
**Thresholds breached:** none yet: length 500 = 500. Any further line puts it over
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-23 | 500 | first detection: 498 at base `00abd1554` (#706, which closed the previous record of this name at 498, from 511), +2 in #707 (`openProjectSetup` destructured from `useSessionsDrawerInspector` and passed as `SessionDrawer.onSetUpProjectHome`). Split deferred with the developer's consent: #706 also touches this file, so a split inside the `#visible-failures` stack turns into a conflict |

## What would close it

#706 already moved resume/delete/terminate (`useSessionsDrawerPaneActions`) and the notices out, so
the old handler-block seam is gone. Two seams remain, both mechanical:

- The eleven per-service `useDaemonClient(...)` calls and their comments (lines 72–100, ~29 lines at the top of
  the component) into a `useSessionsDrawerClients()` hook returning the clients.
- The selection and create handlers still inline (`handleSelectSession`, `handleChildSessionStarted`,
  `handleCreateSession`, `handleCancelCreate`, `handleSessionCreated`, `handleUnknownHome`) into a
  `useSessionsDrawerSelection` hook beside the other `useSessionsDrawer*` hooks.

In the same split, move `openProjectSetup` out of `useSessionsDrawerInspector` (whose name no longer
covers it) into its own hook, e.g. `useProjectSetupNavigation`. Hand TS split: green baseline
(`SessionsDrawerAcceptance`, `SessionsDrawerProjectHomeAcceptance`,
`SessionsDrawerBulkDeleteAcceptance`, `ProjectHomeSetupLinkAcceptance` specs) before, same green
after. Land it after the `#visible-failures` stack, not inside it.
