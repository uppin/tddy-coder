# 2026-10-06 — `plan.rs`, `plan/codec.rs` and `item_anchor.rs` back under the 500-line budget

**Type:** Refactor

A mechanical restructure: the three `tddy-code-restructuring` engine files the same-crate moves had
taken past the 500-production-line budget are split along the seam their growth made, using the
engine's own `move_item` into child modules. Each old path keeps a `pub use` / `pub(crate) use` facade,
so no other file in the repository changes. Layout:
[README.md](../README.md#where-the-code-lives), [item-anchors.md](../item-anchors.md#where-the-code-lives).

## What changed

- `plan.rs` → `plan/refactor_kind.rs`: `RefactorKind` and its `impl` (the variants and the predicates),
  behind `pub use refactor_kind::RefactorKind;`. `RefactorOp` stays in `plan.rs`.
- `plan/codec.rs` → `plan/codec/file_hint.rs` (`hint_of`, `rfc3339`, facade `named`) and
  `plan/codec/groups.rs` (`refuse_split_groups`, re-pointed in place).
- `item_anchor.rs` → `item_anchor/package_lookup.rs` (`owning_package`, `repo_root_hint`,
  `collect_package_files`, facade `named`).

Every public path (`tddy_code_restructuring::{RefactorKind, Reexport, …}`, `item_anchor::owning_package`,
`plan::hint_of`) resolves as before. The diff touches only the three sources, the four children and the
code-issue record. No behaviour change, no new test, no new crate edge.

## Before and after

Production lines counted to the first `#[cfg(test)]` that opens a `mod` (the `check --budget` rule):

| File | Before | After | New child |
|---|---:|---:|---|
| `plan.rs` | 520 | 333 | `plan/refactor_kind.rs` (191) |
| `plan/codec.rs` | 514 | 455 | `plan/codec/file_hint.rs` (47), `plan/codec/groups.rs` (28) |
| `item_anchor.rs` | 517 | 458 | `item_anchor/package_lookup.rs` (71) |

`restructure check --budget 500` reports every file the plan names within 500 production lines. The
suite is unchanged by name (1,136 passed, the baseline); `cargo clippy -p tddy-code-restructuring
--all-targets -- -D warnings` and `cargo fmt --check` are clean; `restructure verify --against` the base
reports no statement lost. Measured 2026-10-06.

## Deferred with the change

The copied-header tidy is unreachable on a resumed run, so three `TODO(sharpen)` markers record the
hand-pruned unused imports in `plan/codec/file_hint.rs`, `plan/codec/groups.rs`, and the `#[cfg(test)]`
facade line in `plan/codec.rs` (`rfc3339`'s only outside user is `plan.rs`'s test module).

## Backlog

Resolved and deleted: `2026-10-05-restructure-engine-files-past-the-500-line-budget` (three
`tddy-code-restructuring` files crossed the 500-production-line budget). Constraints that held and still
bind: `2026-10-04-restructure-move-item-copies-the-whole-use-header`,
`2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures`,
`2026-09-24-restructure-apply-leaves-the-lint-gate-red`,
`2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan` (open).

## Code issues

| Record | Measurement |
|---|---|
| `dead-code-plan-filehint-modified` | the write site, `hint_of`, moved to `plan/codec/file_hint.rs:8,13` (was `plan/codec.rs:219`); 0 read sites outside tests, unchanged; stays open |
| `oversized-file-backends-rust`, `oversized-file-test-binary`, `complexity-rust-facade-lines`, `broken-restructure-anchors-empty-outline` | untouched by this change |
