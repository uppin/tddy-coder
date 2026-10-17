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

## Measurement update 2026-10-17 — clone passphrase prompt

**Regressed, split deferred with consent:** 785 → 795 production lines. +10: one import and one `<StartPassphrasePrompt>` mount (with its comment) beside the start-phase indicator; no logic was added to the pane. The change's own
modules are all small and new; only this unavoidable wiring landed in the file. Not claimed by any PR.
Deferred in `docs/dev/todo/2026-10-17-oversized-files-this-change-grew-by-wiring.md`.
