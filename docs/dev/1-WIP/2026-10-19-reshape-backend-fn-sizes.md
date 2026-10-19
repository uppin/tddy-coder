# Changeset: the Rust backend's functions past 60 lines are cut

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: Behaviour-preserving restructure (function length)
**Stack**: `#reshape` 21/24, branch `feature/reshape/backend-fn-sizes`, base `feature/reshape/extension-outline`, green wave 2.

## Initial Discovery

[2026-10-19-reshape-backend-fn-sizes-initial-discovery.md](2026-10-19-reshape-backend-fn-sizes-initial-discovery.md)

## Prerequisites

- [2026-10-09-functions-of-41-to-60-lines-in-tddy-code-restructuring.md](../todo/2026-10-09-functions-of-41-to-60-lines-in-tddy-code-restructuring.md)
  — *unrelated to the fix*: 41–60-line functions stay deferred.

## Affected Packages

- `packages/tddy-code-restructuring/src/backends/**`; the function-length gate test from `fn-sizes-rest` (K18).

## Related Feature Documentation

- PRD: [PRD-2026-10-19-reshape-backend-fn-sizes.md](../../ft/coder/1-WIP/PRD-2026-10-19-reshape-backend-fn-sizes.md)

## Summary

Extend K18's function-length gate to `src/backends/` and cut every function there longer than 60 lines with engine
operations. Replaces the closed #616.

## Responsibility

- The gate's scope (`src/backends/**` included).
- One engine plan per oversized function; before/after counts recorded below at the contract commit.

### The rules (the contract)

- **R1** — the gate fails on any function over 60 lines in `src/` of `tddy-code-restructuring`.
- **R2** — every cut is `extract_method` / `extract_module` through `tddy-tools restructure`; a refusal stops and asks;
  a post-move build fix is the only hand edit and gets a todo.

## Boundaries

- No behaviour change; `tddy-zed-extensions` out of scope (PRD F1 a); 41–60-line functions stay.

## Dependencies

- Real edges: `language-support` (K11 — construction moves into `RustSupport`), `move-impl-members` (K12),
  `tests-follow` (K13), `move-widen` (K14), `detach-method` (K15) — each adds or reshapes backend functions; measuring
  before them plans cuts on moving code.
- `feature/reshape/extension-outline` (K20) — line position only.

## Draft PR contract

- The gate extended (it fails today, listing the functions), the list of functions over 60 lines **re-measured at this
  commit** with their counts (#616 counted 17 at its base).

## Green wave

**Wave:** 2 of 4.
**Greenable independently:** no — needs K11–K15 green.
**Concurrent with:** `extension-outline` (K20).
**Blocks:** none.

## Successor PRs

`feature/reshape/extension-language-server` (K22).

## Scope

- [ ] USER REVIEW (ticked at `/green` start)
- [ ] Gate extended
- [ ] Each function cut (list at the contract commit)
- [ ] Code-issue records updated with the final measurement
- [ ] Code Quality

## Testing plan

The gate test plus the unchanged package suite.

## Refactoring Needed

_None yet._

## Validation Results

_Not run._
