# oversized-file: backends/rust/imports.rs — import repair and function-local `use` carry in one module

**Location:** `packages/tddy-code-restructuring/src/backends/rust/imports.rs`
**Category:** oversized-file
**Detected:** 2026-10-02 by the `/pr-wrap` file-length gate on #542
**Metrics:** **664 production lines** (2026-10-02, #542; 457 before) · budget 500
**Restructure:** required
**Status:** Open — detected 2026-10-02; #542 (`#live-plan` 5/7) pushed it past the budget

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-02 | 664 | #542: the function-local `use` carry: `function_local_uses_reaching`, `local_uses_to_carry`, `carry_function_local_uses`, `enclosing_function`, `body_of_function`. Merge base 457 → 664, counted to the first `#[cfg(test)]` |

## Why it was not split in #542

The `/pr-wrap` stack-overlap stop. **#543 (`#live-plan` 6/7 `check-parity`) also touches this file**, and
it is stacked directly on #542 — splitting it here would rewrite paths under a PR already in flight and
turn its diff into a conflict.

## What would close it

A decomposition on a follow-up branch **after the `#live-plan` stack lands**, not inside it, driven by
`restructure` (`check --budget 500`). The obvious seam is the function-local `use` carry, which shares nothing with the cross-crate import passes.
