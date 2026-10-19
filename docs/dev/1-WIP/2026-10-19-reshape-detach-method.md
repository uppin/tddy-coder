# Changeset: `detach_method` turns an inherent impl's members into free functions and re-points their callers

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: Enhancement (one new Rust restructure operation)
**Stack**: `#reshape` 15/24, branch `feature/reshape/detach-method`, base `feature/reshape/move-widen`, green wave 1.
**PR**: [#677](https://github.com/uppin/tddy-coder/pull/677) (draft).

## Initial Discovery

[2026-10-19-reshape-detach-method-initial-discovery.md](2026-10-19-reshape-detach-method-initial-discovery.md)

## Prerequisites

- [2026-10-17-restructuring-acceptance-suite-pays-a-rust-analyzer-start-per-test.md](../todo/2026-10-17-restructuring-acceptance-suite-pays-a-rust-analyzer-start-per-test.md)
  — *during*: the new live suite is capped at ~2 rust-analyzer tests (2026-10-17 rebalance rule); the rest run at
  library level.

## Affected Packages

- `packages/tddy-code-restructuring` — `backends/rust/detach_method/` (new), `plan/refactor_kind.rs`, `plan` codec,
  `backends/rust.rs` (`SUPPORTED`, one dispatch arm), `verify/` (`--detach`).
- `packages/tddy-index-daemon`, `packages/tddy-tools` — the `--detach` carrier (`VerifyRequest` field 6).

## Related Feature Documentation

- PRD: [PRD-2026-10-19-reshape-detach-method.md](../../ft/coder/1-WIP/PRD-2026-10-19-reshape-detach-method.md)
- [rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md)

## Summary

The operation, its refusals, its run note and `verify --detach`, as approved for #615 on 2026-10-09, without applying
it to `RustBackend` (PRD F1 a).

## Background

Carved from the closed #615 (`backend-session`). Reuses node 6's self-mode rewrite (`read_fields_through`).

## Responsibility

| Symbol | Where |
|---|---|
| `RefactorKind::DetachMethod` (`detach_method`) | `plan/refactor_kind.rs` |
| `detach_method::{resolve, check, findings}` and the `DetachServer` seam (`span`, `outline`, `references`, `hover`), `impl DetachServer for RustBackend` | `backends/rust/detach_method.rs` (`pub mod`) + children |
| The plan-line refusals (`detach_fields::rules`) | `plan/codec/detach_fields.rs` |
| `verify` declaration `Detach { ty, name }`, `Declared.detaches`, `Declared::with_detaches`, R-detach (`detach::account`); CLI `--detach TYPE=NAME`, `runner::Options.detaches`, `VerifyRequest.detaches` (field 6) | `verify/detach.rs`, `verify/retarget.rs`, `restructure_args.rs`, `runner/{options,comparison}.rs`, index-daemon proto + `cli.rs` + `queries.rs`, `tddy-tools` `index_client.rs` |

### The rules (the contract)

- **R1 — signature.** `&mut self` → `name: &mut T`, `&self` → `name: &T`, receiver lifetime kept; associated functions
  keep their parameters; doc comments, attributes and visibility unchanged; same module.
- **R2 — body.** `self` → `name` (node 6's rewrite), `Self` → the block header's self type.
- **R3 — placement.** Functions follow the block in member order; an emptied block is replaced by them.
- **R4 — callers.** Every server-reported reference: `R.m(args)` → `m(R, args)` (place receiver borrowed `&mut R` /
  `&R`, a reference passed as is, arguments byte for byte); `Self::m`/`T::m` → `m`; from another module `M::m(…)` plus
  a `use` of `M` when the file does not bind it; comments not edited (counted in the note).
- **R5 — refusals** (`check --deep` and `apply`, file and line each): by-value `self`; generic `impl` or `where`;
  trait-impl member; `pub` member; associated const/type; `name` already in the body; the function's name already
  declared or imported in the module; a `Box<T>`/`Rc<T>`/other receiver type (named); a reference inside a macro.
  Plain `check`: missing/malformed `name`, wrong anchor kind, unknown field.
- **R6 — note.** `detach_method: T::a, T::b became functions over `session`; N calls in M files re-pointed (K `use`
  added); the block was emptied and removed`.
- **R7 — verify `--detach TYPE=NAME`** implies `--rebind NAME`; pairs receiver, call and `Self::` forms; excuses one
  lost `impl TYPE {` header per emptied block; a changed argument is still reported.

## Boundaries

- Does **not** detach `RustBackend`'s own operations (dropped with #615).
- Does **not** change `inline_method`, `retarget_impl`, `repoint_call`, `read_fields_through`.

## Dependencies

- `feature/reshape/move-widen` (#604, K14) — **no real edge** (line position). Textual overlap: `backends/rust.rs`
  `SUPPORTED` and dispatch.
- Merged `#reshape` 6 (`methods-leave-type`) — the self-mode rewrite this reuses.

## Draft PR contract

- Surface: `RefactorKind::DetachMethod` (in `SUPPORTED`, routed through `same_crate_dispatch`), the public
  `backends::rust::detach_method` module — `resolve`/`check`/`findings` and `impl DetachServer for RustBackend` as
  `todo!()` (`TODO(reshape-detach-method)`) — `detach_fields::rules` as `todo!()` for `detach_method` lines only, and the
  `--detach` flag carried through `RestructureVerifyArgs.detach` → `Options.detaches` → `Declared::with_detaches`, and
  through `VerifyRequest.detaches` (daemon `cli.rs`/`queries.rs`, `tddy-tools` `index_client.rs`), into a `todo!()`
  `Detach::from_str` and R-detach pass (`detach::account`, a no-op when nothing is declared).
- **The library seam.** The crate has no fake LSP that answers per-test references or hovers (`fake_lsp` returns fixed
  values), so the library suite follows the `crate_move` pattern (`ModuleReferences` + `AKnownReferenceSet`): the
  operation asks a `DetachServer` trait for the anchor's span, the `documentSymbol` outline, the reference set and the
  receiver hover, in the server's own shapes; `RustBackend` answers from rust-analyzer, the test answers from recorded
  answers (`ARecordedServer` in `tests/detach_method.rs`).
- Failing tests (all fail on this node's own `todo!()`s):
  - `tests/detach_method.rs` (library level, recorded server answers, 17 tests):
    `detaches_two_members_and_removes_the_emptied_block`, `re_points_a_call_in_another_module_and_adds_the_use`,
    `borrows_a_place_receiver`, `passes_a_reference_receiver_as_it_is`, `rewrites_self_and_capital_self_in_the_body`;
    one per R5 refusal — `refuses_a_member_that_takes_self_by_value`,
    `refuses_a_member_of_a_generic_impl_or_of_one_with_a_where_clause`, `refuses_a_member_of_a_trait_impl`,
    `refuses_a_pub_member`, `refuses_an_associated_const`, `refuses_a_parameter_name_the_body_already_writes`,
    `refuses_a_function_name_the_module_already_declares`, `refuses_a_call_whose_receiver_is_a_box_naming_its_type`,
    `refuses_a_call_inside_a_macro_invocation` (each: `check` and `resolve` both name `file:line` and the reason);
    `plain_check_reports_a_missing_name_without_a_server`,
    `plain_check_refuses_an_anchor_that_names_no_members_and_a_field_it_does_not_define`,
    `the_note_counts_calls_files_and_uses`.
  - `tests/detach_method_acceptance.rs` (live rust-analyzer, 2; in the `rust-analyzer` nextest group and
    `rust-e2e.filterset`): `a_detached_block_compiles_with_its_callers`, `check_deep_refuses_a_pub_member`.
  - `tests/verify_detach.rs` (library, `verify::compare_with`): `verify_holds_for_a_detach_run`,
    `verify_reports_a_changed_argument`.
- Green on purpose: `tddy-index-daemon` `cli.rs` `verify_carries_the_detaches_it_is_told_of_beside_the_rebinds` (the
  carrier is plumbing, landed whole with the field).

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes.
**Concurrent with:** every other wave-1 node (K11–K14, K16–K19).
**Blocks:** `backend-fn-sizes` (K21) — it measures backend functions after this node adds its own.

## Successor PRs

`feature/reshape/anchors-outline` (#609, K16).

## Scope

- [x] Draft-PR contract: surface published, builds and lints clean (`-p tddy-code-restructuring -p tddy-index-daemon
  -p tddy-tools`)
- [x] Failing acceptance tests written and verified failing on this node's `todo!()`s
- [x] TDD Red — failing library tests (`tests/detach_method.rs`, `tests/verify_detach.rs`)
- [ ] USER REVIEW of the acceptance tests (ticked at `/green` start)
- [ ] Operation (R1–R6)
- [ ] `verify --detach` (R7) and its carriers
- [ ] Docs: `### detach_method` and `## Verify` in rust-code-restructuring.md
- [ ] Code Quality

## Technical changes

### State A (Current)

No operation converts a method into a function with its callers; `SUPPORTED` has 27 kinds (K12 added `move_impl_members`).

### State B (Target)

`detach_method` supported; `SUPPORTED` has 28.

## Testing plan

Library level against recorded LSP answers for every rule; two live tests for the end-to-end compile and one refusal.

## Refactoring Needed

_None yet._

## Validation Results

**Wave 2 contract (2026-10-19), scoped to the three packages this node touches:**

- `cargo clippy -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools --all-targets -- -D warnings`: clean.
- `cargo fmt … -- --check`: clean.
- `cargo test -p tddy-index-daemon --lib --test dual_transport_acceptance`: 58 + 16 pass. `cargo test -p tddy-tools --lib`:
  85 pass.
- `cargo test -p tddy-code-restructuring --lib`: 12 failures, **none this node's** — the parents' contract-stage
  `todo!()`s: 6 `impl_move::assemble` (K12 `move-impl-members`), 3 `source_scan::test_declarations` (K13
  `tests-follow`), 3 `language::tests` (K11 `language-support`). Every other unit test passes, the verify ones included.
- The existing verify suites (`verify_accounts_for_a_rebind`, `_retarget`, `_declared_repoints`, `_member_move`) and the
  plan-line suites (`read_fields_through_plan_lines`, `retarget_impl_plan_lines`) pass with the R-detach pass in the
  pipeline.
- This node's 21 tests fail on its own `todo!()`s: `detach_method::check` (9), `detach_method::resolve` (6),
  `detach_fields::rules` (2 plain-check tests and both live tests, which read the plan through the runner),
  `Detach::from_str` (2 verify tests).
