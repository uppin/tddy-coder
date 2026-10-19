# Changeset: `detach_method` turns an inherent impl's members into free functions and re-points their callers

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: Enhancement (one new Rust restructure operation)
**Stack**: `#reshape` 15/24, branch `feature/reshape/detach-method`, base `feature/reshape/move-widen`, green wave 1.

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
| `detach_method::resolve`, `detach_method::check`, `detach_method::findings` | `backends/rust/detach_method.rs` + children |
| `verify` declaration `Detach { ty, name }`, CLI `--detach TYPE=NAME`, `VerifyRequest.detach` (field 6) | `verify/detach.rs`, `restructure_args.rs`, index-daemon proto + client |

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

- Surface: `RefactorKind::DetachMethod`, the `detach_method` module with `resolve`/`check`/`findings` as `todo!()`
  (`TODO(reshape-detach-method)`), the `--detach` flag and `VerifyRequest.detach` wired to a `todo!()` verify pass.
- Failing tests:
  - `tests/detach_method.rs` (library level, against recorded LSP answers from the crate's fake-LSP fixtures):
    `detaches_two_members_and_removes_the_emptied_block`, `re_points_a_call_in_another_module_and_adds_the_use`,
    `borrows_a_place_receiver`, `passes_a_reference_receiver_as_it_is`, `rewrites_self_and_capital_self_in_the_body`,
    one test per R5 refusal, `plain_check_reports_a_missing_name_without_a_server`, `the_note_counts_calls_files_and_uses`.
  - `tests/detach_method_acceptance.rs` (live rust-analyzer, ≤2): `a_detached_block_compiles_with_its_callers`,
    `check_deep_refuses_a_pub_member`.
  - `tests/verify_detach.rs`: `verify_holds_for_a_detach_run`, `verify_reports_a_changed_argument`.

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes.
**Concurrent with:** every other wave-1 node (K11–K14, K16–K19).
**Blocks:** `backend-fn-sizes` (K21) — it measures backend functions after this node adds its own.

## Successor PRs

`feature/reshape/anchors-outline` (#609, K16).

## Scope

- [ ] USER REVIEW of the acceptance tests (ticked at `/green` start)
- [ ] Operation (R1–R6)
- [ ] `verify --detach` (R7) and its carriers
- [ ] Docs: `### detach_method` and `## Verify` in rust-code-restructuring.md
- [ ] Code Quality

## Technical changes

### State A (Current)

No operation converts a method into a function with its callers; `SUPPORTED` has 26 kinds.

### State B (Target)

`detach_method` supported; `SUPPORTED` has 27.

## Testing plan

Library level against recorded LSP answers for every rule; two live tests for the end-to-end compile and one refusal.

## Refactoring Needed

_None yet._

## Validation Results

_Not run._
