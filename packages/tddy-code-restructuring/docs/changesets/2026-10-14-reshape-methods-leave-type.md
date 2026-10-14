# 2026-10-14 — `read_fields_through`, `retarget_impl`'s forwarding delegator, and the `verify` rules for both

**Type:** Feature

`#reshape` 6/19 (`feature/reshape/methods-leave-type`, PR #603). Behaviour:
[read-fields-through.md](../read-fields-through.md), [retarget-impl.md](../retarget-impl.md).

## What changed

- **Plan surface** — `RefactorKind::ReadFieldsThrough` (`plan/refactor_kind.rs`); `plan/codec/rebind_fields.rs`
  (new) holds RP1-RP4; `plan/codec/retarget_fields.rs` enforces P9 (`leave_delegator` and `expr` go
  together); `plan/codec/signature_fields.rs` admits `expr` on the new operation. No `RefactorOp` field
  was added. `SUPPORTED` 25 → 26 (`backends/rust.rs`).
- **`read_fields_through`** — `backends/rust/read_fields_through.rs` (the run, the field-mode trial text) and
  `read_fields_through/{range,method,typing,edits}.rs`. `range` and `edits` are `pub(crate)`. RS1-RS4 are
  static findings; RS5-RS7 and the typing use the server.
- **Delegator** — `backends/rust/retarget_impl/delegator.rs` (S7, the forwarding text, the dead-delegator
  note); `retarget_impl.rs` loses `deferred_delegator` and gains `the_replacement`;
  `retarget_impl/rewrite.rs` gains `Layout::with_delegators`.
- **`verify`** — `verify/rebind.rs` (new: `Rebind`, R-rebind) and R3 in `verify/retarget.rs`;
  `Declared.rebinds` with `with_rebinds`; one pass in `verify.rs`. The `--rebind` carrier:
  `RestructureVerifyArgs.rebind`, `runner::Options.rebinds`, `runner/comparison.rs`.
- **Tests** — `read_fields_through_plan_lines.rs`, `read_fields_through_acceptance.rs` (live),
  `retarget_impl_delegator_acceptance.rs` (live), `verify_accounts_for_a_rebind.rs`, plus cases in
  `retarget_impl_plan_lines.rs` and `verify_accounts_for_a_retarget.rs`. The two live binaries are
  registered in `.config/nextest.toml` and `.config/rust-e2e.filterset`.

## Verified (scoped to `tddy-code-restructuring`, `tddy-index-daemon`, `tddy-tools`)

All 29 acceptance tests and the surface unit tests pass; `read_fields_through_acceptance` 10/10,
`retarget_impl_delegator_acceptance` 6/6, `retarget_impl_acceptance` 13/13 as the regression pin.
`cargo check --all-targets`, `cargo clippy -D warnings` and `cargo fmt --check` are clean. CI answers for
the rest of the workspace.

## Known limit

In self mode a generic `impl`'s `Self::X` is written `W<T>::X`, not valid in expression position; the
compile gate refuses it and no test covers it.

## Backlog and code issues

- Resolved and deleted: "`restructure` has no operation that makes a method read its fields through a
  borrowed state value" (`2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter`)
  and "`restructure` cannot re-point a call's receiver or leave a forwarding delegator"
  (`2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator`). The `pub`
  wrapper case of the second moved to `2026-10-09-restructure-dead-pub-wrappers-are-not-reported`.
- **Final measurements.**
  - `backends/rust.rs`: wiring only, 3,045 → 3,056 production lines counted to the first inline
    `#[cfg(test)] mod`; by the `/pr-wrap` awk (to the first `#[cfg(test)]` attribute) 543 → 545. Recorded
    in `code-issues/oversized-file-backends-rust.md`, which stays open.
  - New files, production lines: `read_fields_through.rs` 420, `read_fields_through/range.rs` 327,
    `retarget_impl/delegator.rs` 326, `verify/rebind.rs` 198; all under 500. Longest new functions
    are 42-56 lines.
  - `verify/retarget.rs`: 341 → 555 production lines (R3). **Over budget; the split is deferred** with the
    developer's consent (2026-10-14) because `#reshape` 18/19 (`backend-session`) and `fn-sizes-backend`
    edit the file. Recorded in `code-issues/oversized-file-verify-retarget.md` and
    `2026-10-14-restructure-verify-retarget-rs-exceeds-the-500-line-budget`.
