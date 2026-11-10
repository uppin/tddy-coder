# complexity: spawn_pty_writer

**Location:** `packages/tddy-pty/src/runtime.rs:449` — `spawn_pty_writer`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) wrap re-measurement (brace matching, fn line to closing brace; nesting read by hand as control-flow depth)
**Metrics:** **40 lines** (2026-11-10) · **nesting depth 5** (match > loop > select > match > if; brace depth 9 counting the thread closure and `block_on` block)
**Thresholds breached:** nesting 5 > 4 (`/analyze-clean-code`); length within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 wrap); unclaimed

Deferred by the user, 2026-11-10 (#876 wrap): left for a later refactoring node

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 40 | 5 | new in #876: extracted from `open_pty_and_pump` (Tier C), carrying the writer thread's depth with it. `spawn_pty_reader` beside it (33 lines, control-flow depth 3) is within budget |

## What would close it

Extract the stdin forwarding loop (the `Ok(mut w)` arm) into its own function. Anchor with `tddy-tools restructure anchors`, never by hand, then prove the seam with
`restructure check --deep` against a warm index.
