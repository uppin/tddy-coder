# oversized-file: SessionsDrawerScreen.tsx

**Location:** `packages/tddy-session-web/src/SessionsDrawerScreen.tsx`
**Category:** oversized-file
**Detected:** 2026-11-14 — `/pr-wrap` step 3.5 file-length gate on #886 (`#orchestrators` 9/11), production lines counted as the whole file (TypeScript)
**Metrics:** **581 production lines** of 581 total · budget 500 · **1.16× over**
**Thresholds breached:** length 581 > 500
**Restructure:** not designed
**Status:** Open — deferred under the stack-branch rule (the drawer files are stack-shared; n11 also edits this area) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-14 | 487 → 581 | **Pushed past budget** by #886 (`#orchestrators` 9/11): the detail pane gains the project-stack route case and the drawer's Stacks row wiring. Not split here: stack-shared file, deferred to the post-stack sweep |

## What would close it

Hand split (TypeScript): move the detail-pane route cases (composer, stack view, session views) into a sibling module, leaving the drawer screen composition in place. Prove with the package's Cypress component specs green before and after.
