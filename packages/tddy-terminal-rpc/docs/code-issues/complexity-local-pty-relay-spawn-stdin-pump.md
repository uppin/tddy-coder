# complexity: spawn_stdin_pump

**Location:** `packages/tddy-terminal-rpc/src/local_pty_relay.rs:101` — `spawn_stdin_pump`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) wrap re-measurement (brace matching, fn line to closing brace; nesting read by hand as control-flow depth)
**Metrics:** **65 lines** (2026-11-10) · nesting depth 4 (loop > select > match > if)
**Thresholds breached:** length 65 > 60 (`/analyze-clean-code`); depth at the budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 wrap); unclaimed

Deferred by the user, 2026-11-10 (#876 wrap): left for a later refactoring node

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 65 | 4 | new in #876: extracted from `run` (Tier C, 141/8 → 53/3); it spawns both the stdin pump and the SIGWINCH resize pump. `spawn_stdout_pump` beside it (23 lines, control-flow depth 3) is within budget |

## What would close it

Split the resize pump into its own function, leaving the stdin pump alone. Anchor with `tddy-tools restructure anchors`, never by hand, then prove the seam with
`restructure check --deep` against a warm index.
