# oversized-file: backends/rust/early_return.rs — early-return refusal and the unit-tail rule in one module

**Location:** `packages/tddy-code-restructuring/src/backends/rust/early_return.rs`
**Category:** oversized-file
**Detected:** 2026-10-02 by the `/pr-wrap` file-length gate on #542
**Metrics:** **531 production lines** (2026-10-02, #542; 465 before) · budget 500
**Restructure:** required
**Status:** Open — detected 2026-10-02; #542 (`#live-plan` 5/7) pushed it past the budget

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-02 | 531 | #542: `ends_in_a_unit_tail`, `declares_a_return_type` and `function_header` for the unit-tail refusal. Merge base 465 → 531, counted to the first `#[cfg(test)]` |

## Why it was not split in #542

The `/pr-wrap` stack-overlap stop. **#543 (`#live-plan` 6/7 `check-parity`) also touches this file**, and
it is stacked directly on #542 — splitting it here would rewrite paths under a PR already in flight and
turn its diff into a conflict.

## What would close it

A decomposition on a follow-up branch **after the `#live-plan` stack lands**, not inside it, driven by
`restructure` (`check --budget 500`). The obvious seam is the header reading (`function_header`, `declares_a_return_type`, `opens_a_function_body`, `matching_open_brace`), which `imports.rs` also reads.
