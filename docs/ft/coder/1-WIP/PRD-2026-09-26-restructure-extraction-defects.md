# Extractions refuse or repair what rust-analyzer gets wrong, and never hang - PRD

**Date**: 2026-09-26
**PRD Type**: Bug Fix

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `extract_method`
  and `extract_variable` preconditions and output repair; the refusal classes they report.
- **Related Feature**: [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md) — a hung
  extraction no longer wedges the warm workspace.

## Summary

Four recorded extraction defects: two leave a tree that does not compile, one is a `check --deep`
that passes an extraction the apply then breaks, and one waits forever and wedges the warm daemon.
This PRD makes each either **refused before any edit**, with the class that names the cause, or
**repaired** by the engine, with a bounded wait everywhere.

## Background

- [`…-extract-variable-waits-forever-on-a-range-opening-with-a-borrow.md`](../../dev/todo/2026-09-25-restructure-extract-variable-waits-forever-on-a-range-opening-with-a-borrow.md)
  — the type probe hovers on the range's first character; on `&` the hover stays `null` for ever.
- [`…-extract-variable-hoists-a-borrowed-field-by-value.md`](../../dev/todo/2026-09-25-restructure-extract-variable-hoists-a-borrowed-field-by-value.md)
  — `&self.v` extracted as `let x = self.v;` (`E0507`).
- [`…-extract-method-accepts-a-return-before-a-unit-if-tail.md`](../../dev/todo/2026-09-25-restructure-extract-method-accepts-a-return-before-a-unit-if-tail.md)
  — rust-analyzer rewrites the early `return` into `ControlFlow`; `E0433` after apply.
- [`…-extract-method-leaves-a-function-local-use-behind.md`](../../dev/todo/2026-09-25-restructure-extract-method-leaves-a-function-local-use-behind.md)
  — the new function names a type only a function-local `use` in the origin imported.

## Proposed Changes

### What's Changing

1. **Type probe on a hover-bearing token.** `extract_variable` probes at the first token of the range
   that can carry a hover, skipping leading `&`, `&mut`, `*`, `!`, `-`, `(`.
2. **Bounded probe.** Once the index is ready, a hover that stays `null` beyond the ready-index
   answer bound ends as `rust-analyzer's answer was unusable:`, naming the position. No extraction
   waits without a bound.
3. **Borrowed place.** When the selection is a non-`Copy` place whose parent is `&<place>` /
   `&mut <place>`, the selection is widened to the borrow, so the binding is `let x = &self.v;`.
   An assist answer that still hoists a non-`Copy` place out of `&self` by value is refused as
   `rust-analyzer's answer was unusable:`.
4. **Unit tail after an early return.** The tail-range exception requires a non-`()` tail; a range
   containing a `return` and ending in a `()` tail is refused as `this seam cannot be cut here:`,
   naming the `return` lines and advising to start after the early exit — in `check` as well as
   `apply`.
5. **Function-local `use`.** After an `extract_method` whose new function is not nested in the
   origin, each unresolved name a function-local `use` of the origin binds gets that `use` added to
   the new function's body (kept local); it is dropped from the origin when nothing there still names
   it. `check` reports the carry statically.

### What's Staying the Same

- Every other operation, and extraction's successful paths.
- The `ControlFlow` rewrite is not "repaired": the range is refused.

## Impact Analysis

### Technical Impact

`packages/tddy-code-restructuring/src/backends/rust.rs` and `backends/rust/` (probe, preconditions,
import pass over the new function); tests in `tests/extract_variable_acceptance.rs`,
`tests/extract_method_control_flow_acceptance.rs`, `tests/extract_method_signature_acceptance.rs`,
`tests/wedged_request_acceptance.rs`.

### User Impact

Four hand fixes and one wedged daemon restart stop being needed.

## Acceptance Criteria

- [ ] `extract_variable` over `&self.v` in `fn f(&self) -> bool { let r = &self.v; r.is_some() }`
      resolves (or refuses) within the ready-index bound and never hangs.
- [ ] A probe whose hover stays `null` past the bound ends as `rust-analyzer's answer was unusable:`
      naming the position, and the warm workspace answers the next request.
- [ ] Extracting `self.v` from `if let Some(s) = &self.v { … }` yields `let x = &self.v;` and
      compiles.
- [ ] `fn f(x: Option<u32>) { let Some(v) = x else { return; }; if v > 1 { … } }` extracted from the
      `let` to the end is refused by `check --deep` and by `apply` before any edit, naming the
      `return` line.
- [ ] An extraction whose range names a type bound by a function-local `use` compiles after apply,
      with the `use` carried into the new function, and the origin keeps it only if still used.

## References

- [Rust code restructuring](../rust-code-restructuring.md)
- [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md)
