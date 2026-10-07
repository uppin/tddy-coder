# 2026-10-07 — `retarget_impl` moves an inherent `impl`'s members to another type, and `verify` accounts for it

**Type:** Feature

A new Rust restructuring operation, `retarget_impl`, rewrites an inherent `impl`'s self type to
another type of the same crate: the whole block, or the run of members it is anchored on (the block
is split at the run). It re-points the paths the old type named for the moved members, adds one
`use`, refuses before any write when a moved member reads a field the new type does not declare, and
keeps every comment. `restructure verify` is taught to account for a declared retarget, on a
declaration the author makes. Behaviour:
[rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) and
[retarget-impl.md](../../../packages/tddy-code-restructuring/docs/retarget-impl.md).

## What changed

- **The operation (`tddy-code-restructuring`, `backends/rust/retarget_impl.rs` and `retarget_impl/`,
  new)** — the plan-line schema (`RetargetImpl` in `plan/refactor_kind.rs`; `to_type` on `RefactorOp`),
  the parse-time refusals P1-P8 in `plan/codec/retarget_fields.rs`, the static findings P7/P8 in
  `retarget_impl/preflight.rs`, and the `resolve` arm. The block split (`rewrite.rs`), the `Old::` to
  `New::` path re-points from the server's own reference set, the field refusal S4/S5 (`fields.rs`),
  S1-S3/S6 (`outline.rs`) and the one `use` (`imports.rs`) are all here. `SUPPORTED` 22 → 23.
- **`verify` accounting (`verify/retarget.rs`, new)** — `Declared` / `Retarget` read `OLD=NEW`; R1
  (rename pairing) pairs a lost statement with a gained one equal to it once every whole-identifier
  `OLD` becomes `NEW`, and R2 excuses the `impl` header lines a split repeats. Both count into
  `Excused::repointed`. `verify::compare` stays and delegates to `verify::compare_with`.
- **The carrier** — `--retarget OLD=NEW` (`RestructureVerifyArgs.retarget`, `Options.retargets`),
  `VerifyRequest.retargets` (proto field 3), through `tddy-tools`'s `index_client.rs::verify` and the
  daemon's `cli.rs` / `queries.rs`.
- **Tests** — `tddy-code-restructuring` gains `tests/retarget_impl_plan_lines.rs` (library),
  `tests/verify_accounts_for_a_retarget.rs` (library) and `tests/retarget_impl_acceptance.rs` (live
  rust-analyzer, `cargo check` as the assertion); `tddy-index-daemon`
  `tests/dual_transport_acceptance.rs` pins the declaration through the CLI and the daemon.

## Code issues

| Record | Measurement |
|---|---|
| `tddy-code-restructuring` `oversized-file-backends-rust.md` | **Grown, not closed**: wiring only (`mod retarget_impl;`, `SUPPORTED` 22 → 23, one `check` arm, one `resolve` arm). All logic went to `backends/rust/retarget_impl/` and its children. History row appended; record kept — the split is deferred because the sibling `#sharpen` nodes also edit this file |

## Backlog

Resolves `docs/dev/todo/2026-10-05-restructure-no-operation-retargets-an-impl-to-another-type.md` —
`retarget_impl` is the operation the entry asked for. Entry deleted.

Narrows (keeps) `docs/dev/todo/2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md`
— the **delegator half** (`leave_delegator`) was cut to a follow-up at the M3 decision point (the
block split alone grew the module past its budget), so `variant: "leave_delegator"` and `expr` are
published in the schema and **refused** by the engine rather than implemented. The **receiver half**
(`repoint_call`) is [#594](https://github.com/uppin/tddy-coder/pull/594)'s; the entry is deleted only
when both halves have landed.

## Stack

Node 6 of 8 of the `#sharpen` stack. Depends on `tidy-engine-files` (#588, merged) for the
`plan/refactor_kind.rs` home of `RefactorKind`. Dependents:
[#594](https://github.com/uppin/tddy-coder/pull/594) `repoint-call` consumes the `verify`
declaration carrier this node builds; [#595](https://github.com/uppin/tddy-coder/pull/595)
`repoint-facade`.
