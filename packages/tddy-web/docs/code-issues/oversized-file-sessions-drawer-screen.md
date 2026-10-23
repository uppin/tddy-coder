# oversized-file: SessionsDrawerScreen.tsx

**Location:** `packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx`
**Category:** oversized-file
**Detected:** 2026-10-23 by the `/pr-wrap` step 3.5 file-length gate on PR #699 (`#project-home` 4/7)
**Metrics:** **511 lines** (whole file, TS) · budget 500 · **1.02× over** — `wc -l packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx`
**Thresholds breached:** length 511 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-23 | 511 | first detection — 497 at base `2273795b6` (#698), +14 in #699: the two hook imports, `useSessionsDrawerProjectHomes`, `useEnsureProjectHome` and the two props passed to `SessionDrawer`. Split deferred because `#project-home` 5/7–7/7 (#700–#702) build on this screen — the stack rule: no split lands mid-stack |

## What would close it

The screen is already a composition of `useSessionsDrawer*` hooks; what is left inline is the
handler block (`handleSelectSession`, `handleResume`, `handleBulkDelete` and the delete-confirm
state). Moving that block into a `useSessionsDrawerHandlers` hook beside the others takes the file
well under budget. Hand TS split: green baseline (`SessionsDrawerAcceptance`,
`SessionsDrawerProjectHomeAcceptance`, `SessionsDrawerBulkDeleteAcceptance` specs) before, same
green after. Land it after the `#project-home` stack, not inside it.
