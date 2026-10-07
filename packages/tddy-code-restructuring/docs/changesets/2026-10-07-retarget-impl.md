# 2026-10-07 — `retarget_impl` moves an inherent `impl`'s members to another type

**Type:** Feature

A new Rust operation, `retarget_impl`, rewrites an inherent `impl`'s self type to another type of the
same crate: the whole block, or the run of members it is anchored on, in which case the block is
split in place at the run. It re-points the paths the old type named for the moved members, adds one
`use`, refuses before any write when a moved member reads a field the new type does not declare, and
keeps every comment and attribute. Behaviour:
[retarget-impl.md](../retarget-impl.md).

## What changed

- **Plan surface** — `RefactorKind::RetargetImpl` (`plan/refactor_kind.rs`); `RefactorOp.to_type:
  Option<String>` (`plan.rs`), with `to_type: None` on the full `RefactorOp` literals; the refusals
  P1-P8 in `plan/codec/retarget_fields.rs` (new), wired through `plan/codec.rs`; `SUPPORTED` 22 → 23
  (`backends/rust.rs`).
- **The operation** — `backends/rust/retarget_impl.rs` and its children: `preflight.rs` (the static
  P7/P8 findings), `outline.rs` (the member-level outline read, S1-S3), `rewrite.rs` (the header
  rewrite, the block split, the `Old::` → `New::` re-points), `fields.rs` (the field refusal S4/S5),
  `imports.rs` (the one `use`, S6). `item_move/text.rs`'s `use_insertion` and `scope_of` widen to
  `pub(in crate::backends::rust)`.
- **`verify`** — `verify/retarget.rs` (new) holds `Declared` / `Retarget` and the two rules R1
  (rename pairing) and R2 (repeated-header accounting), run between visibility pairing and re-point
  pairing and counted into `Excused::repointed`. `verify::compare` delegates to the new
  `verify::compare_with`. The declaration travels as `RestructureVerifyArgs.retarget` →
  `Options.retargets` → `runner::verify`.
- **Tests** — `tests/retarget_impl_plan_lines.rs` (library), `tests/verify_accounts_for_a_retarget.rs`
  (library) and `tests/retarget_impl_acceptance.rs` (live rust-analyzer, `cargo check` as the
  assertion; registered in `.config/nextest.toml` and `.config/rust-e2e.filterset`).

## Code issues

| Record | Measurement |
|---|---|
| `oversized-file-backends-rust.md` | **Grown, not closed**: wiring only in `backends/rust.rs`; all logic in `backends/rust/retarget_impl/`. History row appended; record kept |

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-07-retarget-impl.md).
