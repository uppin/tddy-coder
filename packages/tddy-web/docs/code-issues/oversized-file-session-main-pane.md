# oversized-file: SessionMainPane.tsx

**Location:** `packages/tddy-web/src/components/sessions/SessionMainPane.tsx`
**Category:** oversized-file
**Detected:** 2026-10-04 by the `/pr-wrap` step 3.5 file-length gate on PR #571 (`#live-plan` 12/15)
**Metrics:** **674 lines** (whole file, TS) · budget 500 · **1.35× over** — `wc -l packages/tddy-web/src/components/sessions/SessionMainPane.tsx`
**Thresholds breached:** length 674 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-04 | 674 | first detection — already over before #571 (666 at base `c3567fde`); +8 in #571. Deferred because dependent #572 touches this file (the stack rule: no split lands mid-stack) |

## What would close it

Not yet analysed. A skim shows the `SessionMainPaneProps` interface (lines 57 to 179, about 120 lines, one client prop per service) ahead of the one exported component `SessionMainPane` (from line 180), so the props and client type aliases are a plausible first seam to move into a sibling file. What the component body would split into was not examined. The `code-restructuring` engine is Rust-only, so this is a hand TS split: green baseline before, mechanical moves only. Land it after #572 and the stack, not inside it.
