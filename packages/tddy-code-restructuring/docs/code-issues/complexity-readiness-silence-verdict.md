# complexity: silence_verdict

**Location:** `packages/tddy-code-restructuring/src/backends/rust/readiness.rs:79` — `silence_verdict`
**Category:** complexity
**Detected:** 2026-10-17 by `/analyze-clean-code` on #607 (`#reshape` 9/19); re-derive by reading
the body (`sed -n '79,99p' packages/tddy-code-restructuring/src/backends/rust/readiness.rs`)
**Metrics:** 21 lines · **nesting depth 3** (`if loading` → `if silent_past` / `else` → `match ready_silent_for`) · 4 parameters · pure function, unit-tested
**Thresholds breached:** none at the must-refactor level; flagged because a guarded `match` would make it 1
**Restructure:** no — a body rewrite, ordinary work
**Status:** Open — **unclaimed**
**Verified:** by hand, 2026-10-17

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-10-17 | 21 | 3 | first detection; new in #607 |

## What the tool found

The loading and ready branches are two nested `if`/`else` levels with a `match` inside the second,
where one `match (loading, ready_silent_for)` with guards on `silent_past` would decide the three
verdicts (`Waiting`, `ReadyUntypable`, `Stalled`) flat.

## Why it matters here

It is the single decision every bounded wait on rust-analyzer goes through (the wait for type
inference and the crate-index warm-up), so it is read whenever a wait is refused or not.

## What would close it

Rewrite the body as one guarded `match`; the unit tests (`a_loading_server_whose_words_are_unchanged_past_the_loading_bound_is_stalled`,
`a_loading_server_with_new_words_inside_the_bound_is_waited_on` and the rest of `readiness`' tests)
pin the behaviour. Deferred in #607: a body rewrite was not one of the edit kinds the stack rule
allowed in a file later `#reshape` branches carry.

## Verified by hand

- 2026-10-17 (#607 wrap): body read; nesting as stated.
