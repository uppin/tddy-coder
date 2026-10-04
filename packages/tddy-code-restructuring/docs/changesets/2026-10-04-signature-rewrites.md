# 2026-10-04 — Signature and call-site operations

**Type:** Feature

`#live-plan` 15/15, PR [#567](https://github.com/uppin/tddy-coder/pull/567). Product entry:
[2026-10-04-signature-rewrites.md](../../../../docs/ft/coder/changelog/2026-10-04-signature-rewrites.md).

Eight operations change a function's signature and, one call at a time, its callers:
`change_param_type`, `add_param`, `reorder_params`, `change_return_type`, `add_call_arg`,
`remove_call_arg`, `change_call_arg` and `reorder_call_args`. How it works:
[signature-rewrites.md](../signature-rewrites.md).

Resolved backlog entry: `2026-09-24-restructure-has-no-signature-operations` ("`restructure` has no
operations that change a function's signature"). Adding, reordering and retyping parameters and changing
a return type are delivered as per-call-site operations composed in a transactional group, not as one
`change_signature` that fans out to callers; renaming a parameter (through `rename_symbol` with a range
anchor) is documented in the plan schema.

- **`plan.rs`:** `RefactorKind::{ChangeParamType, AddParam, ReorderParams, ChangeReturnType,
  AddCallArg, RemoveCallArg, ChangeCallArg, ReorderCallArgs}`, `RefactorKind::edits_a_call_site`,
  `RefactorOp.{type_, expr, order}` (serde `type`), `OrderKey`.
- **`plan/rust_syntax.rs`:** `one_type`, `one_expr` (`syn`, exactly one `Type` / `Expr`, no statement
  inside an `expr`), `permutation`.
- **`plan/codec/signature_fields.rs`:** the per-operation field refusals, raised when the plan is read.
- **`backends/rust/signature_rewrites.rs`** with `signature_rewrites/declaration.rs` and
  `call_site.rs`: exact-span edits over masked code. **`backends/rust/return_type.rs`:**
  `return_type_assist`, the assist behind `change_return_type`'s `variant`.
- **`Cargo.toml`:** `syn` 2 (`full`, `visit`), the workspace's version, added with the developer's
  approval.

## Final measurements

| File | Production lines |
|---|---|
| `backends/rust/signature_rewrites.rs` | 502 before the split, 170 after (children `declaration.rs` ~185, `call_site.rs` ~175 re-measured at the wrap) |
| `backends/rust/return_type.rs` | 49 (moved out of `rust.rs`) |
| `plan/codec.rs` | 563 before, 437 after (child `plan/codec/signature_fields.rs`, 131) |
| `backends/rust.rs` | 2,706 before this PR, 2,831 after |

**Deferred, with the developer's consent:** the 125 lines still added to `backends/rust.rs` — the impl
members `rewrite_signature`, `wrap_or_unwrap_return_type`, `offered_assist` and two resolve branches.
The engine refused to move them because rust-analyzer cannot put a `mod` inside an `impl` body. The
record `docs/code-issues/oversized-file-backends-rust.md` stays, with this PR's row; its remainder is
the impl-member seam. The five unit tests stay in `signature_rewrites.rs` because `extract_module` would
nest a moved module inside `tests`.

`dead-code-plan-filehint-modified.md` was re-measured (0 read sites) and is unchanged.

Tests: `tests/signature_rewrites_acceptance.rs` (live rust-analyzer, `--test-threads=1`) plus unit tests
in `plan.rs`, `plan/rust_syntax.rs`, `backends/rust/return_type.rs` and `backends/rust/signature_rewrites.rs`;
the scoped `./test -p tddy-code-restructuring` ran 44 test binaries with 0 failures before the last
base rebase.
