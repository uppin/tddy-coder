# oversized-file: CreateSessionPane.tsx

**Location:** `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` step 3.5 file-length gate on PR #740 (`#start-progress` 7/8)
**Metrics:** **506 lines** (non-test source, whole file) · budget 500 · **1.012× over**
**Thresholds breached:** length 506 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 498 | base of PR #740 (under budget) |
| 2026-10-24 | 502 | +4 in PR #740: the start's host label (`startingHostId` / `startingHostLabel`) for the step-list heading. Split deferred with the developer's consent, #start-progress 7/8 — the file is shared with the stack's node 8/8 (Cancel), so a split here would cascade into its diff |
| 2026-10-24 | 506 | +4 in PR #741 (`#start-progress` 8/8): the `taskClient` prop, passed with `sessionToken` into `useCreateSessionSubmit`, and `startCancel` handed to `CreateSessionActions`. **Deferred with the developer's consent, #start-progress 8/8** |

## What would close it

Hand TS split after the `#start-progress` stack lands: move the host resolution (`startingHostId`,
`startingHostLabel`, the split-codebase choice) into a `useStartingHost` hook beside `useCreateSessionCatalogs`.
Green baseline before (`CreateSessionPane.cy.tsx`, `CreateSessionStartProgress.cy.tsx`), mechanical move only.
