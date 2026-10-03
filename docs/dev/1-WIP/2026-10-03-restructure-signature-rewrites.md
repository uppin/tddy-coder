# Changeset: Signature and call-site operations for transactional groups

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-restructure-signature-rewrites-initial-discovery.md).

## Stack

`#live-plan` 15/15 — branch `feature/live-plan/signature-rewrites`, base `feature/live-plan/session-restructure-tools`.
PR: [#567](https://github.com/uppin/tddy-coder/pull/567)

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- Operations `change_param_type`, `add_param`, `reorder_params`, `change_return_type` (declaration only; `change_return_type` `variant` = `wrap_result`/`wrap_option`/`unwrap` via assists).
- Call-site operations `add_call_arg`, `remove_call_arg`, `change_call_arg`, `reorder_call_args`, each anchored on one call expression (item anchor + relative range).
- Fields `type`, `expr` (each parsed with `syn` as exactly one `Type`/`Expr`) and `order`; their refusals.
- Adding `syn` (workspace version 2) to `tddy-code-restructuring`.

## Boundaries

- Does **not** fan a change out to callers — every call site is its own operation.
- Does **not** implement groups, gates or rollback (`transactional-groups`).
- Does **not** add the caller-rewriting assists (`signature-assists`).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `transactional-groups` (8) | `group` field, group-end gate, rollback | every acceptance test applies a signature change + call-site ops as one group | change group semantics or the journal |

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

**Sequencing facts (recorded at the red phase, 2026-10-03):**

- Every acceptance test except `change_return_type_with_a_type_edits_the_declaration` and
  `a_call_site_op_whose_range_is_not_a_call_is_refused` applies its ops as **one group**, so once
  this node's own stubs are implemented those five reach `transactional-groups`' group gate
  (`runner::group_gate::{gate_group, roll_back_group}`) and journal pre-images (`PreImage::capture`
  / `restore`), which are still `todo!` on this branch. They go green only after
  `transactional-groups` is green; today they fail earlier, on this node's own stubs.
- The two ungrouped acceptance tests and every unit test depend on nothing of a parent's.
- `RefactorOp` literals in the parent nodes' tests (`transactional_groups_acceptance.rs`,
  `signature_assists_acceptance.rs`) and in `src/` gained `type_: None, expr: None, order: Vec::new()`
  — forced by the new fields, no behaviour changed.

## Draft PR contract

The first push after this commit publishes: `RefactorKind::{ChangeParamType, AddParam, ReorderParams, ChangeReturnType, AddCallArg, RemoveCallArg, ChangeCallArg, ReorderCallArgs}`; `RefactorOp.{type_, expr, order}` (serde `type`; `order: Vec<OrderKey>`, re-exported); `RefactorKind::edits_a_call_site`; `plan::rust_syntax::{one_type, one_expr, permutation}`; backend `SUPPORTED` entries, resolve arms and `return_type_assists` — `TODO(signature-rewrites)`; failing tests below. Pushed as commit 2.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — its acceptance tests need `transactional-groups`' gate to compile at the group's end; its parse/validation unit tests are greenable now.
**Concurrent with:** plan-dialog, session-restructure-tools
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

None — the top of the stack.

## Prerequisites

### ✅ RESOLVED HERE — signature operations — [`2026-09-24-restructure-has-no-signature-operations.md`](../todo/2026-09-24-restructure-has-no-signature-operations.md)

By the time this node wraps, `signature-assists` (9) has narrowed the entry to the caller-breaking half; this node completes it and its wrap deletes the file.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md) — eight operations, `type`/`expr`/`order` fields validated with `syn`, declaration and call-site edits

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-10-03-signature-rewrites.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

No operation changes a signature (`backends/rust.rs` `SUPPORTED`, 10 ops); `CODE_BEARING_FIELDS` refuses `text`/`code`/`content`; `rename_symbol` shows the overlay + `minimal_edits` pattern for engine-authored edits; `references_at` lists call sites; `wrap_return_type`/`unwrap_return_type` assists exist but leave callers to the compiler; `syn` 2 is a workspace dependency of `tddy-code-analysis` only.

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

All written and run 2026-10-03; every one ❌ fails, on this node's own stubs. Acceptance run:
`./dev cargo test -p tddy-code-restructuring --test signature_rewrites_acceptance -- --test-threads=1`.

### tddy-code-restructuring — `tests/signature_rewrites_acceptance.rs` (live rust-analyzer)

Fixture: a one-crate `ledger` workspace — `pricing::label(count: u32) -> String`, `pricing::total(price, quantity, discount)` and `pricing::product(price, quantity) -> u32`, each called from `checkout::basket` and/or the crate root's `sample`. Call-site ops use item anchors on the caller with a relative range on its one call expression (line 2 of the item).

| Test | Group | Fails today on |
|---|---|---|
| ❌ `a_param_type_change_and_its_call_site_changes_compile_as_one_group` — `change_param_type` (`count: &str`) + one `change_call_arg` per caller; files exact, compiles | `retype` | `rust_syntax::one_type` `todo!` (parse) |
| ❌ `a_group_missing_one_call_site_is_rolled_back_naming_its_error` — same group without the crate root's call; refusal starts `group `retype` does not compile at its end, so it was rolled back:` and carries `mismatched types` in `src/lib.rs`; tree unchanged | `retype` | `one_type` `todo!` |
| ❌ `reorder_params_and_reorder_call_args_compile_as_one_group` — `order: ["discount","price","quantity"]` + `reorder_call_args` `order: [3,1,2]` per caller; files exact, compiles | `reorder` | backend `TODO(signature-rewrites)` refusal |
| ❌ `add_param_and_add_call_arg_compile_as_one_group` — `add_param` `unit: &str` `last` + `add_call_arg` `"items"` `last` per caller | `unit` | `one_type` `todo!` |
| ❌ `change_return_type_wrap_result_uses_the_assist` — `variant: wrap_result` (the assist wraps the tail in `Ok`) then `type: Result<u32, String>` to name the error type the assist leaves open | `fallible` | `one_type` `todo!` |
| ❌ `change_return_type_with_a_type_edits_the_declaration` — `-> String` to `-> impl std::fmt::Display`; body and caller untouched, compiles | — | `one_type` `todo!` |
| ❌ `a_call_site_op_whose_range_is_not_a_call_is_refused` — `change_call_arg` ranged on `basket`'s name; refusal says `is not a call expression`, nothing written | — | `rust_syntax::one_expr` `todo!` |

### tddy-code-restructuring — unit (`plan.rs`, `plan/rust_syntax.rs`)

- ❌ `plan::tests::a_type_that_is_not_one_rust_type_is_refused` — `"u32 u32"` → `plan is malformed: `type` must be exactly one Rust type, and `u32 u32` is not` (`one_type` `todo!`)
- ❌ `plan::tests::an_expr_that_is_not_one_expression_is_refused` — `"total(1); total(2)"` (`one_expr` `todo!`)
- ❌ `plan::tests::an_expr_carrying_a_statement_is_refused` — `{ let unit = "items"; unit }` → `` `expr` may carry no statement`` (`one_expr` `todo!`)
- ❌ `plan::tests::reads_a_parameter_type_change_naming_its_parameter_and_type` — the JSON `type` reads into `type_` (`one_type` `todo!`)
- ❌ `plan::rust_syntax::tests::order_must_name_every_parameter_once` — a missing and a repeated parameter are refused naming it (`permutation` `todo!`)
- ❌ `plan::rust_syntax::tests::an_order_naming_every_parameter_once_maps_each_new_position_to_its_old_one` (`permutation` `todo!`)

## Technical Debt & Production Readiness

**Stubs (`TODO(signature-rewrites)`), all in `tddy-code-restructuring`:**

- `plan::rust_syntax::{one_type, one_expr, permutation}` — bodies `todo!`. `parse_op` already calls
  `one_type`/`one_expr` on any `type`/`expr` (plumbing only), so every plan carrying either panics
  until they are implemented.
- `parse_op` — the per-operation field refusals (missing `name`/`type`/`variant`/`expr`/`order`,
  `type`+`variant` together on `change_return_type`, a call-site op without an item anchor carrying a
  relative range, a field on an op that cannot honour it) are a TODO comment, not code: writing them
  would make no red test pass, but they are behaviour, so they are left to green.
- `backends/rust.rs` `resolve_opening` — the eight new kinds are in `SUPPORTED` and refused before a
  server starts with `TODO(signature-rewrites): <Kind> is not implemented yet`.
- `backends/rust.rs` `return_type_assists(variant)` — declares the `wrap_result` / `wrap_option` /
  `unwrap` assists (titles `wrap return type in result|option`, `unwrap result|option return type`,
  kind `refactor.rewrite`, caret on the return type, single file). **Unverified** against the bundled
  rust-analyzer, and unused until green (`#[allow(dead_code)]`). It is keyed by variant, not
  `RefactorKind`, so it is not an `assist_for` arm.

**Not written, and why:**

- No `parse_op` validation unit tests beyond the four `type`/`expr` ones — the developer's list
  names only those plus `order`; the TODO above lists what green should pin.
- `order` validation is a function the backend calls once it has read the declaration or call
  (`permutation`), not a `parse_op` check: whether `order` names *every* parameter needs the
  parameter list, which the plan does not carry.
- `plan-schema.md` (skill reference) and the package README are not updated — wrap work.

## Decisions & Trade-offs

- **`order` is `Vec<OrderKey>`**, `#[serde(untagged)] enum OrderKey { Position(u32), Name(String) }`:
  parameter names for `reorder_params`, one-based argument positions for `reorder_call_args`, as the
  PRD's table says, in one field.
- **Call-site positions** (`variant` on the call-site ops) are `first`, `last` or a one-based index;
  `add_param`'s are `first`, `last` or `after:<param>`, per the PRD.
- **`syn = { version = "2", features = ["full", "visit"] }`** — the developer approved the
  dependency; `visit` is what finds a statement nested anywhere in an `expr`. Same spec as
  `tddy-code-analysis`.
- **`wrap_result` names no error type** (rust-analyzer writes `_` without snippets), so the acceptance
  test pairs it with a `type` change in the same group — the composition groups exist for.


## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## TODO

- [x] Record initial discovery (`2026-10-03-restructure-signature-rewrites-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests (`./test -p <pkg>` per affected package); CI for the rest
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-restructure-signature-rewrites-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
