# oversized-file: CreateSessionPane.tsx

**Location:** `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx`
**Category:** oversized-file
**Detected:** 2026-10-04 by the `/pr-wrap` step 3.5 file-length gate on PR #571 (`#live-plan` 12/15)
**Metrics:** **785 lines** (whole file, TS) · budget 500 · **1.6× over** — `wc -l packages/tddy-web/src/components/sessions/CreateSessionPane.tsx`
**Thresholds breached:** length 785 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-04 | 785 | first detection — already over before #571 (783 at base `c3567fde`); +2 in #571. Deferred because dependent #572 touches this file (the stack rule: no split lands mid-stack) |

## What would close it

Not yet analysed. A skim shows one exported component, `CreateSessionPane` (from line 70), carrying about 33 `useState` / `useEffect` / `useCallback` sites, so the first candidate seam is extracting its state groups into custom hooks beside the file. The `code-restructuring` engine is Rust-only, so this is a hand TS split: green baseline before, mechanical moves only, same green after. Land it after #572 and the stack, not inside it.
