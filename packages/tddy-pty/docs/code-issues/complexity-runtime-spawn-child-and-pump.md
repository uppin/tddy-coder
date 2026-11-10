# complexity: spawn_child_and_pump

**Location:** `packages/tddy-pty/src/runtime.rs:304` — `spawn_child_and_pump`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) wrap re-measurement (brace matching, fn line to closing brace; nesting read by hand as control-flow depth)
**Metrics:** **67 lines** (2026-11-10) · nesting depth 3
**Thresholds breached:** length 67 > 60 (`/analyze-clean-code`); depth within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 wrap); unclaimed

Deferred by the user, 2026-11-10 (#876 wrap): left for a later refactoring node

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 67 | 3 | new in #876: extracted from `open_pty_and_pump` (Tier C), which fell 179/9 → 40 |

## What would close it

Split the child spawn from the exit-monitor thread it starts. Anchor with `tddy-tools restructure anchors`, never by hand, then prove the seam with
`restructure check --deep` against a warm index.
