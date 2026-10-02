# 2026-10-02 — Extraction defects

**Type:** Fix

`#live-plan` 5/7, PR [#542](https://github.com/uppin/tddy-coder/pull/542). Product entry:
[2026-10-02-extraction-defects.md](../../../../docs/ft/coder/changelog/2026-10-02-extraction-defects.md).
Single-package change, so no cross-package entry.

Four recorded extraction defects are each refused before any edit or repaired, and no extraction
waits without a bound.

- **Probe position** (`backends/rust/selection.rs`, `hover_bearing_position`): `extract_variable`
  hovers at the first position of the range past leading `&`, `&mut`, `*`, `!`, `-`, `(`.
- **Probe bound** (`backends/rust/readiness.rs`, `READY_HOVER_BOUND`, `silent_past`): a hover that
  stays `null` for 30 s once the index is ready ends as `rust-analyzer's answer was unusable:`.
- **Borrowed place**: a single-line place under `&` / `&mut` is widened to the borrow
  (`widened_to_its_borrow`); `refuse_by_value_hoist` refuses any place under a borrow the selection
  omits. The refusal is **structural, not type-based**: no `Copy` check, so a `Copy` place under a
  borrow is refused as well, and a borrow on another line is refused rather than widened.
- **Unit tail** (`backends/rust/early_return.rs`, `ends_in_a_unit_tail`): a range holding a `return`
  and running to the end of a `()` function is refused in `check`, `check --deep` and `apply`. The
  `ControlFlow` rewrite is not repaired. The test is on the function header text.
- **Function-local `use`** (`backends/rust/imports.rs`, `carry_function_local_uses`): the `use` items
  of the origin function that bind a name the range mentions are written into the new function.
  The origin's `use` is kept, which can leave an unused-import warning. `check` reports the carried
  items on its progress line, not as a finding.

## Backlog entries resolved (deleted from `docs/dev/todo/`)

- `2026-09-25-restructure-extract-variable-waits-forever-on-a-range-opening-with-a-borrow` —
  `extract_variable` waits for ever on a range that opens with `&`. The server-side cancellation on
  client disconnect named in its third bullet was out of scope; the bound makes a stuck probe end by
  itself.
- `2026-09-25-restructure-extract-variable-hoists-a-borrowed-field-by-value` —
  `extract_variable` hoists a field read under `&` by value.
- `2026-09-25-restructure-extract-method-accepts-a-return-before-a-unit-if-tail` —
  `extract_method` accepts a `return` in a range that ends with a unit `if`.
- `2026-09-25-restructure-extract-method-leaves-a-function-local-use-behind` —
  `extract_method` leaves a function-local `use` behind.

`2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures` is a different
defect in the same operations and stays in the backlog.

## Code issues

Production lines, counted to the first `#[cfg(test)]`, merge base to this PR:

| File | Before | After | Outcome |
|---|---|---|---|
| `backends/rust.rs` | 4,433 | 4,475 | regressed; deferred, #543 also touches it. Record `oversized-file-backends-rust.md` |
| `backends/rust/early_return.rs` | 465 | 531 | crossed the 500 budget; deferred for the same reason. Record `oversized-file-backends-rust-early-return.md` |
| `backends/rust/imports.rs` | 457 | 664 | crossed the 500 budget; deferred for the same reason. Record `oversized-file-backends-rust-imports.md` |

Decomposition belongs on a follow-up branch after the `#live-plan` stack lands.

## Tests

`tests/extraction_defects_acceptance.rs` (live rust-analyzer): a range opening with a borrow
resolves within the ready bound; a borrowed field binds the borrow and compiles; an early return
ending in a unit `if` is refused with the file unchanged; a function-local `use` is carried. Unit
tests in `selection.rs` (position, widening, by-value refusal), `readiness.rs` (`silent_past`),
`early_return.rs` (`ends_in_a_unit_tail`) and `imports.rs` (`function_local_uses_reaching`).

Not covered by a test: a hover that stays `null` past the bound against a real server (the only
natural source was the `&` range, which the probe position removes), and the `check` progress line
for carried `use` items.
