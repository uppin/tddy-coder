# oversized-file: WorkQueueView.tsx

**Location:** `packages/tddy-workqueue-web/src/WorkQueueView.tsx`
**Category:** oversized-file
**Detected:** 2026-11-23 — `/pr-wrap` step 3.5 file-length gate on #1015 (`#workqueue` 14/14), whole file counted (TypeScript)
**Metrics:** **501 lines** · budget 500
**Thresholds breached:** length 501 > 500
**Restructure:** not designed
**Status:** Closed — `#workqueue` 15/15 sweep (#1022): 489 (whole TypeScript file) lines — pure mutation-state helpers moved to `workItemMutationState.ts`. Record deleted at wrap.
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam is a first reading

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-23 | 484 → 501 | **Crossed the budget** in #1015: the Land wave state (`useLandPropagate`, the wave count read off the status stream) and the `LandPropagateContext` provider around the item list. Not split: the file is stack-shared (`#workqueue` nodes 3–13 edited it), so the stack-branch rule defers the split to the post-stack sweep |

## What would close it — candidate seam (not proven)

Lift the label filter (state, options and `<select>`) and the start-session dialog state into hooks of
their own beside `useWorkItemLinksState` — about 80 lines out.
