# oversized-file: SessionsDrawerScreen.tsx

**Location:** `packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx`
**Category:** oversized-file
**Detected:** 2026-10-23 by `/pr-wrap` on PR #707 (`#visible-failures` 2/2)
**Metrics:** **490 lines** (whole file, TS) · budget 500 · **0.98×** — `wc -l packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx`
**Thresholds breached:** none: 490 < 500, ten lines of headroom
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-23 | 500 | first detection: 498 at base `00abd1554` (#706, which closed the previous record of this name at 498, from 511), +2 in #707 (`openProjectSetup` destructured from `useSessionsDrawerInspector` and passed as `SessionDrawer.onSetUpProjectHome`). Split deferred with the developer's consent: #706 also touches this file, so a split inside the `#visible-failures` stack turns into a conflict |
| 2026-10-24 | 490 | `project-home-setup-pane`: the per-service clients moved to `useSessionsDrawerClients` (−33), the setup pane's wiring added (+23); `openProjectSetup` is gone from `useSessionsDrawerInspector` |

## What would close it

#706 already moved resume/delete/terminate (`useSessionsDrawerPaneActions`) and the notices out, so
the old handler-block seam is gone. Two seams remain, both mechanical:

- ~~The eleven per-service `useDaemonClient(...)` calls into a `useSessionsDrawerClients()` hook~~ —
  done 2026-10-24.
- The selection and create handlers still inline (`handleSelectSession`, `handleChildSessionStarted`,
  `handleCreateSession`, `handleCancelCreate`, `handleSessionCreated`, `handleUnknownHome`) into a
  `useSessionsDrawerSelection` hook beside the other `useSessionsDrawer*` hooks.

(`openProjectSetup` left `useSessionsDrawerInspector` on 2026-10-24; the setup pane's routing is
`useProjectHomeSetup`.) Hand TS split: green baseline
(`SessionsDrawerAcceptance`, `SessionsDrawerProjectHomeAcceptance`,
`SessionsDrawerBulkDeleteAcceptance`, `ProjectHomeSetupLinkAcceptance` specs) before, same green
after. Land it after the `#visible-failures` stack, not inside it.
