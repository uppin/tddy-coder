# oversized-file: SessionDrawer.tsx

**Location:** `packages/tddy-web/src/components/sessions/SessionDrawer.tsx`
**Category:** oversized-file
**Detected:** 2026-10-23 by the `/pr-wrap` step 3.5 file-length gate on PR #697 (`#project-home` 2/7)
**Metrics:** **534 lines** (whole file, TS) · budget 500 · **1.07× over** — `wc -l packages/tddy-web/src/components/sessions/SessionDrawer.tsx`
**Thresholds breached:** length 534 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-23 | 534 | first detection — 453 at base `ad78a7426`, +81 in #697 (per-project grouping: `SessionGroupBody`, the collapsed-set state and the selection auto-expand effect). Split deferred because #698 and #699 (`#project-home` 3/7, 4/7) also touch this file — the stack rule: no split lands mid-stack |

## What would close it

Two seams, both inside this file and both mechanical: `SessionGroupBody` and `SessionPartitionBody` (the Active/Remaining partition rendering, from the `#project-home` 2/7 move) into `SessionDrawerGroupBody.tsx`, and the collapsed-set state with its two effects into a `useProjectGroupCollapse` hook beside `projectGroupCollapse.ts`. Hand TS split: green baseline (`SessionDrawer`, `SessionsDrawer*Acceptance` specs) before, same green after. Land it after the `#project-home` stack, not inside it.
