# complexity: forward_lines_until_exit

**Location:** `packages/tddy-supervisor/src/streamed.rs:112` — `forward_lines_until_exit`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) structural re-measurement
**Metrics:** **20 lines** (2026-11-10) · **nesting depth 5** (loop > select > match > if) · 3 branch/match lines · 3 early `return`s
**Thresholds breached:** nesting 5 > 4 (`/analyze-clean-code`); length and complexity within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 sweep); unclaimed

Deferred by the user, 2026-11-10 (#876 sweep): the restructure engine refused the seam (missing imports for builder types / early returns in the range); revisit after the engine fix

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 20 | 5 | #876 B4f: extracting the loop to the function's tail is refused because the generated signature names the generic `S` without declaring it; the single-line send seam holds an early `return`. |

## What would close it

Extract the forwarding loop (or the one-line send) once the engine declares the generics it
names and handles an early return in the range. Anchor with `tddy-tools restructure anchors`,
never by hand, then prove the seam with `restructure check --deep` against a warm index.
