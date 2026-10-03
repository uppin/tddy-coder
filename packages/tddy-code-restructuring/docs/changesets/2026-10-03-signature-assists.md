# 2026-10-03 — Signature assists: `remove_unused_param` and `convert_tuple_return_to_struct`

**Type:** Feature

`#live-plan` 9/15, PR [#569](https://github.com/uppin/tddy-coder/pull/569). Product entry:
[2026-10-03-signature-assists.md](../../../../docs/ft/coder/changelog/2026-10-03-signature-assists.md).
Single-package change, so no cross-package entry.

Two operations that change a signature together with every caller, resolved through
`multi_file_assist` like `inline_method`. How it works:
[signature-assists.md](../signature-assists.md).

- **`plan.rs` / `plan/codec.rs`:** `RefactorKind::{RemoveUnusedParam, ConvertTupleReturnToStruct}`;
  `parse_op` refuses either without `name`.
- **`backends/rust.rs`:** `SUPPORTED` grows to twelve; `assist_for` rows for both (`remove unused
  parameter` / `refactor`, `convert tuple return type to tuple struct` / `refactor.rewrite`, both
  verified against the bundled rust-analyzer by the acceptance tests); the dispatch in
  `multi_file_assist`.
- **`backends/rust/signature.rs`** (new): the caret on a parameter's name and inside a return type, the
  used-parameter refusal, and the rename that gives the introduced struct the plan's `name`.
- **Plan-schema skill reference:** rows and notes for both operations.

The conversion keeps the function's visibility on the struct and its fields (`pub struct Name(pub A,
pub B);`), which is what rust-analyzer writes.

Backlog: *`restructure` has no operations that change a function's signature* (2026-09-24) is **narrowed,
not resolved** — the assist half is delivered; the caller-breaking half (`change_return_type`,
`change_signature`) remains and is claimed by `signature-rewrites` (#live-plan 15).

## Final measurements

| Measure | Result |
|---|---|
| `tddy-code-restructuring` tests | 747 passed, 0 failed (lib), 4 of 4 `signature_assists_acceptance` (scoped run, rebased on master `0ed7c2aa`) |
| `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings` | clean |
| `backends/rust.rs` production lines | 2,666 → 2,705 (+39: wiring only; over the 500 budget, split deferred — every open `#live-plan` node edits it; record `oversized-file-backends-rust.md`) |
| `plan.rs` / `plan/codec.rs` production lines | 377 → 387 / 375 → 393 (under budget) |
| `backends/rust/signature.rs` production lines | new, 403 |
