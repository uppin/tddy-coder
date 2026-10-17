# signature: process_start takes `&Option<OperationContext>`

**Location:** `packages/tddy-code-restructuring/src/spawn_record.rs:176` — `process_start`
**Category:** signature
**Detected:** 2026-10-17 by `/analyze-clean-code` on #607 (`#reshape` 9/19); re-derive with
`grep -n -A5 'fn process_start' packages/tddy-code-restructuring/src/spawn_record.rs`
**Metrics:** 1 parameter typed `&Option<OperationContext>` · 2 call sites (`spawn_record.rs:145`, `:159`), both passing `&self.operation`
**Restructure:** no — a private signature change, ordinary work
**Status:** Open — **unclaimed**
**Verified:** by hand, 2026-10-17

## Measurement history

| Run | `&Option<_>` parameters | Note |
|---|---|---|
| 2026-10-17 | 1 | first detection; the parameter was added by #607 |

## What the tool found

The idiomatic borrowed optional is `Option<&OperationContext>` (`self.operation.as_ref()` at the call
sites). `&Option<T>` ties the callee to the caller's storage and is what clippy's
`ref_option` (pedantic) names.

## Why it matters here

Minor: it is private and has two callers. It is recorded because the clean-code pass found it on
code #607 wrote and could not change in place.

## What would close it

`operation: Option<&OperationContext>` and `.as_ref()` at both call sites. Deferred in #607: a
signature change was not one of the in-place edit kinds the stack rule allowed in a file later
`#reshape` branches carry.

## Verified by hand

- 2026-10-17 (#607 wrap): signature and both call sites read.
