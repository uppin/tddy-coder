# The Rust backend's functions past 60 lines are cut - PRD

**Date**: 2026-10-19
**PRD Type**: Behaviour-preserving restructure (function length)
**Stack**: `#reshape` 21/24 (`feature/reshape/backend-fn-sizes`, base `feature/reshape/extension-outline`; real parents
`language-support`, `move-impl-members`, `tests-follow`, `move-widen`, `detach-method` — wave 2)

> Replaces the closed `#reshape` 19/19 (#616, `fn-sizes-backend`), which measured after `rust-backend-split` (dropped).
> The function-size goal of the original stack (functions over 60 lines; 41–60 deferred) is kept for `backends/`.

## Affected Features

- None (internal). `packages/tddy-code-restructuring/docs/code-issues/` records the measurement.

## Summary

- The function-length gate that `fn-sizes-rest` (K18) applies outside the backend is extended to `src/backends/`.
- Every backend function longer than 60 lines at this node's base is cut by engine operations (`extract_method`,
  `extract_module`), with no behaviour change. #616 counted 17 (including `into_destination` at 61); the count is
  **re-measured at this node's contract commit**, after K11–K15 have reshaped the backend.

## Background

- K11 moves construction and launch into `RustSupport`; K12, K13, K14 and K15 add or change backend functions. Measuring
  before them would plan cuts on code that moves.
- Every cut is engine-driven; a refusal stops and asks (standing rule).

## Proposed Changes

### What's Changing

- The gate (`fn-sizes-rest`'s test) covers `src/backends/**` too.
- One plan per oversized function, recorded in the changeset with before/after line counts.

### What's Staying the Same

- Every operation's behaviour and output. Functions of 41–60 lines stay (backlog entry
  `2026-10-09-functions-of-41-to-60-lines-in-tddy-code-restructuring.md`).

## Acceptance Criteria

- [ ] The gate finds no function over 60 lines under `src/` (backend included).
- [ ] The package suite passes unchanged.

## Open questions

- **F1 — scope with the new crate.** (a) `tddy-code-restructuring/src/backends/` only (recommended); (b) also
  `tddy-zed-extensions`.

## References

- Closed #616 PRD (`feature/reshape/fn-sizes-backend`).
