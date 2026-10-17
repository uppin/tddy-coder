# duplication: the ` ({id})` op-id suffix formatted twice

**Location:** `packages/tddy-code-restructuring/src/lib.rs:315` — `WrittenByRun::fmt`;
`packages/tddy-code-restructuring/src/runner/entry_points/store_run.rs:243` — `resolving_line`
**Category:** duplication
**Detected:** 2026-10-17 by `/analyze-clean-code` on #607 (`#reshape` 9/19); re-derive with
`grep -rn 'format!(" ({id})")' packages/tddy-code-restructuring/src`
**Metrics:** **2 copies** of `.map(|id| format!(" ({id})")).unwrap_or_default()`, both added by #607
**Restructure:** no — one shared helper, ordinary work
**Status:** Open — **unclaimed**
**Verified:** by hand, 2026-10-17

## Measurement history

| Run | Copies | Note |
|---|---|---|
| 2026-10-17 | 2 | first detection; the stale refusal's "applied operation {n} ({id})" and the progress line's "op {index} ({id}) of {total}" |

## What the tool found

Both texts name an operation as its index followed by an optional parenthesised id, and each builds
the suffix itself.

## Why it matters here

The op id is the join key between a progress line, a spawn-record line and a journal record. Two
spellings of it can drift apart, and a reader joining logs by hand then misses a match.

## What would close it

One helper (beside `OpId`, or a `Display` for an index-plus-optional-id pair) that both call.
Deferred in #607: `lib.rs` and `store_run.rs` are carried by later `#reshape` branches.

## Verified by hand

- 2026-10-17 (#607 wrap): `lib.rs:315` and `store_run.rs:243` hold the same `.map(...)` expression.
