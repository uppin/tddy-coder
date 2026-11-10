# complexity: for_each_line

**Location:** `packages/tddy-supervisor/src/streamed.rs:182` — `for_each_line`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) structural re-measurement
**Metrics:** **23 lines** (2026-11-10) · **nesting depth 5** (loop > match > for > if > if) · 6 branch/match lines · 1 early `return`
**Thresholds breached:** nesting 5 > 4 (`/analyze-clean-code`); length and complexity within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 sweep); unclaimed

Deferred by the user, 2026-11-10 (#876 sweep): the restructure engine refused the seam (missing imports for builder types / early returns in the range); revisit after the engine fix

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 33 → 23 | 6 → 5 | #876 B4e: the emitter is now the free `emit_trimmed_line`. The per-chunk byte loop (the `split_chunk` seam) was refused three times: its range holds an early `return`, which the engine will not carry into an extracted function. |

## What would close it

Extract the per-chunk byte loop once the engine handles an early return in the range (or converts
it to a returned keep-going flag). Anchor with `tddy-tools restructure anchors`, never by hand,
then prove the seam with `restructure check --deep` against a warm index.
