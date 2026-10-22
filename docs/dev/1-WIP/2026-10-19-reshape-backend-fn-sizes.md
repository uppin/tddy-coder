# Changeset: the Rust backend's functions past 60 lines are cut

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: Behaviour-preserving restructure (function length)
**Stack**: `#reshape` 21/24, PR [#680](https://github.com/uppin/tddy-coder/pull/680), branch `feature/reshape/backend-fn-sizes`, base `feature/reshape/extension-outline`, green wave 2.

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

### Published (commit 2)

The gate is extended **beside** K18's, not by editing it: K18 (`fn-sizes-rest`, #613) owns
`LEFT_TO_THE_BACKEND_NODE`, `functions_past_the_cap_under` and `the_backend_is_left_to_its_own_node`, and is itself
contract-stage. This node adds its own items to `packages/tddy-code-restructuring/tests/function_length_budget.rs`,
reusing K18's measurement (`production_files_under`, `functions_of`, `CAP` — `fn` line to closing brace, test code
skipped):

- `backend_functions_past_the_cap_under(src)` — the functions past the cap under `src/backends/`, named relative to
  `src`.
- `no_production_function_in_the_backend_runs_past_sixty_lines` — **red**, listing the 14 functions below.
- `a_long_function_under_the_backend_is_reported_with_its_file_line_and_length`,
  `a_backend_function_of_exactly_sixty_lines_is_within_the_cap` — green on purpose: they pin the helper.

When both K18 and this node are green, the two gates may be folded into one by removing K18's exclusion and its pin
(K18's own `TODO(reshape-19)` names this node by its old number) — whichever node lands second does it.

### Functions past 60 lines in `src/backends/`, re-measured at this commit

Base: `feature/reshape/extension-outline` @ `5f97ed93e4d3`. K11–K15 are contract-stage, so this measures the backend
**before** their moves; `/green` re-measures after rebasing onto them and the gate's output is authoritative.
14 functions (#616 counted 17 at its base):

| # | Function | File:line | Lines |
|---|---|---|---|
| 1 | `choose_import` | `src/backends/rust/import_text.rs:18` | 65 |
| 2 | `next_import` | `src/backends/rust/imports.rs:103` | 62 |
| 3 | `assemble` | `src/backends/rust/item_move/assemble.rs:72` | 82 |
| 4 | `visibilities` | `src/backends/rust/item_move/assemble.rs:207` | 73 |
| 5 | `into_destination` | `src/backends/rust/item_move/assemble.rs:451` | 61 |
| 6 | `edits_for_file` | `src/backends/rust/item_move/sites.rs:95` | 91 |
| 7 | `move_items` | `src/backends/rust/item_move.rs:59` | 73 |
| 8 | `retarget_impl` | `src/backends/rust/retarget_impl.rs:58` | 65 |
| 9 | `assist_for` | `src/backends/rust.rs:299` | 93 |
| 10 | `start` | `src/backends/rust.rs:772` | 87 |
| 11 | `request` | `src/backends/rust.rs:860` | 64 |
| 12 | `offered_assist` | `src/backends/rust.rs:1059` | 71 |
| 13 | `resolve_opening` | `src/backends/rust.rs:1267` | 161 |
| 14 | `assisted_edit` | `src/backends/rust.rs:1498` | 123 |

K18's own gate is also red at this base (9 functions outside `backends/`) — K18's red, not this node's.

## Green wave

**Wave:** 2 of 4.
**Greenable independently:** no — needs K11–K15 green.
**Concurrent with:** `extension-outline` (K20).
**Blocks:** none.

## Successor PRs

`feature/reshape/extension-language-server` (K22).

## Scope

- [ ] USER REVIEW (ticked at `/green` start)
- [x] Gate extended (contract commit)
- [ ] Each function cut (list at the contract commit)
- [ ] Code-issue records updated with the final measurement
- [ ] Code Quality

## Green log (2026-10-22)

Re-measured at green's base, `0e7121919` on `cde6980f7` (K11–K15 merged): the gate's list is 14 functions. `visibilities`
(73 lines at the contract base) is 55 now and is no longer listed, and `survey` (`detach_method/survey.rs:91`, 79 lines) is
listed for the first time. The gate's output is authoritative.

Baseline, scoped (`./test -p tddy-code-restructuring`, before any cut): 1856 passed, 3 failed. The failing set:
`no_production_function_in_the_backend_runs_past_sixty_lines` (this node's red gate), and
`registry::tests::a_files_module_is_answered_by_the_backend_that_reads_its_own_files` and
`registry::tests::a_files_module_through_a_backend_without_a_resolver_is_refused_by_name` (both `not yet implemented:
BackendRegistry::module_of`, red before this node).

Per function:

- `choose_import` — deferred by developer 2026-10-22 (K18 standing rule): the `by_crate` tier cut failed the compile gate
  (E0261, undeclared `'a`); rolled back. Engine todo `2026-10-22-extract-method-respells-dyn-lifetime-and-clone-as-to-owned.md`
  § 7. Exempted in `DEFERRED_BY_BACKEND_FN_SIZES`.
- `retarget_impl` — cut (`f4643810d`, `refuse_moved_fields`). Its verify shape was the `&T` family and the `Ok(())` guard-lift
  tail, both accepted by the developer's 2026-10-22 answers; listed in the commit.
- `start` — cut twice: `spawn_command` (`27f380bc3`) and `initialize_server` (`870f0b0c3`).
- `into_destination` — cut (`7ef339e00`, `imports_block`, the `&mut` and `&T` families).
- `assemble` — cut three times: `changed_files` (`6f41e8f33`), `absorb_doc_links` (`3e2dc76f5`), `names_of_items`
  (`f49023c7c`). Its `Ok(())` tail is the return shape of a `?`-only block, not a guard.
- `move_items` — cut (`eca556a05`, `resolution_of`).
- `assist_for` — three match-arm bodies cut (`3fa026218`, `2b9938947`, `c5aff3bdf`); category (b) each.
- `offered_assist` — two cuts (`8960d2668` `settled_actions`, `b9a290293` `target_range`). The `b9a290293` message says the
  function was 63 lines after the first cut; the gate measured 64, and 57 after the second.
- `resolve_opening` — five tail peels (`e9c029440`, `ce4f11fbb`, `69ece56cb`, `739dee814`, `a398f216d`), each a range to the
  end of the body that holds its early returns (the tail-range exception). Now 54 lines.
- `assisted_edit` — `7f0a7a45f` (`finish_relocation`) cut; the `rename_and_check` cut (`n21-28`) failed its compile gate
  (E0308, engine todo § 11) and was rolled back. Still 110 lines, and the rest of the body needs helpers with more than seven
  parameters. Deferred by developer 2026-10-22 (K18 standing rule), exempted as a whole.
- `survey` — deferred: the tail range was refused by `check --deep` (engine todo § 10). Exempted.
- `next_import` — deferred: the tuple return of the `Lost` pair fails with E0106 (engine todo § 9). Exempted.
- `request` — deferred: the `begin_wait` cut fails with E0502 (engine todo § 8). Exempted.
- `edits_for_file` — deferred by developer 2026-10-22 (`Deferred by developer 2026-10-22`): its only cuts that the engine
  accepts are the loop after `let mut needed` (lines 159–183), whose helper takes ten parameters, two of them closures
  (`in_region`, `in_destination`), and `clippy::too_many_arguments` fires. Exempted; recorded in
  `docs/dev/todo/2026-10-09-restructure-has-no-operation-to-introduce-a-parameter-struct.md`, owned by the sweep node.
- Clippy, scoped (`./dev cargo clippy -p tddy-code-restructuring --all-targets`, without deny): one warning, `let_and_return`
  at `rust.rs:1177`, from `target_range` (`b9a290293`). Engine todo § 13. Fixed by a consented hand edit (developer
  2026-10-22): the body is now the `if` expression, with a `TODO(reshape-sweep)` comment.

## Stop lifted (2026-10-22)

The developer decided both open questions: `edits_for_file` is deferred and exempted, and the `target_range` fix is
approved as a hand edit. No function is left over the cap outside the exemption list.

Deferred by developer 2026-10-22 (K18 standing rule): `choose_import`, `survey`, `next_import`, `request`, `assisted_edit`,
`edits_for_file`.

Verify gap (developer answers 2026-10-22, the `&T` family and the guard-lift tail): accepted and listed per commit above,
recorded in `docs/dev/todo/2026-10-22-verify-does-not-account-for-mut-param-writes-from-extract-method.md`.

## Testing plan

The gate test plus the unchanged package suite.

## Refactoring Needed

_None yet._

## Validation Results

_Not run._
