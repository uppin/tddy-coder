# 2026-10-07 — `repoint_call` re-points a call's callee or every receiver of a method, and `verify` accounts for it

**Type:** Feature

A new Rust restructuring operation, `repoint_call`, rewrites the part of a call in front of its
argument list and keeps the arguments byte for byte: one call's callee (the *single* form), or the
receiver of every call of a method the server knows (the *bulk* form). `restructure verify` is taught
to account for a declared re-point, on a declaration the author makes. Behaviour:
[rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) and
[repoint-call.md](../../../packages/tddy-code-restructuring/docs/repoint-call.md).

## What changed

- **The operation (`tddy-code-restructuring`, `backends/rust/repoint_call.rs` and `repoint_call/`,
  new)** — the plan-line schema (`RepointCall` in `plan/refactor_kind.rs`; `callee` on `RefactorOp`),
  the parse-time refusals in `plan/codec/repoint_call_fields.rs`, the callee parsers
  `rust_syntax::{one_callee, one_receiver_template}`, and the `resolve` arm. The single form
  (`single.rs`) replaces one call's callee from a range anchor, before any server. The bulk form
  (`sites.rs`, `receivers.rs`) reads every reference the server reports for the anchored method,
  classifies each site, refuses every non-method-call at once, and inserts the template's hops after
  each receiver. `SUPPORTED` 23 → 24.
- **`verify` accounting (`verify/repoint.rs`, new)** — `Repoint` reads `OLD=NEW`; **R-call** pairs a
  lost and a gained statement equal once every call of the declared callee is written the new one
  (an `OLD` immediately followed by `(` or `::<`), outside strings, comments and lifetimes. The pairs
  count into `Excused::repointed`; no new wire field.
- **The carrier** — `--repoint OLD=NEW` (`RestructureVerifyArgs.repoint`, `Options.repoints`),
  `VerifyRequest.repoints` (proto field 4), through `tddy-tools`'s `index_client.rs::verify` and the
  daemon's `cli.rs` / `queries.rs`. Carrier plumbing only; no operation is named in `tddy-tools`.
- **Tests** — `tddy-code-restructuring` gains `tests/repoint_call_plan_acceptance.rs` (library),
  `tests/verify_accounts_for_declared_repoints.rs` (library) and `tests/repoint_call_acceptance.rs`
  (live rust-analyzer, `cargo check` as the assertion); `tddy-index-daemon`
  `tests/dual_transport_acceptance.rs` pins the declaration through the CLI and the daemon.

## Code issues

| Record | Measurement |
|---|---|
| `tddy-code-restructuring` `oversized-file-backends-rust.md` | **Grown, not closed**: wiring only (the `RepointCall` `SUPPORTED` entry, one `check` arm, one `resolve` arm — ~11 lines); 2,986 → 2,997 production lines. All logic went to `backends/rust/repoint_call/` and its children. History row appended; record kept — the split is deferred because the sibling `#sharpen` nodes also edit this file |
| `tddy-code-restructuring` `dead-code-plan-filehint-modified.md` | **Unchanged**: this PR added `callee` to `RefactorOp` in `plan.rs` but not near `FileHint::modified`; still one write site, no reader. Row appended; record kept |

## Backlog

Resolves no `docs/dev/todo/` entry. The receiver half of
`docs/dev/todo/2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md`
is delivered here, but its **delegator half** belongs to `retarget_impl`, which claims the whole entry
and keeps it until both halves have landed. The
[state-parameter todo](../todo/2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md)
(`self.<field>` → `state.<field>`) is a field read, not a call, and stays open by decision.

## Stack

Node 7 of 8 of the `#sharpen` stack. Depends on `retarget-impl` (#593) for the `verify` declaration
carrier this node extends, and on `tidy-engine-files` (#588, merged) for the `plan/refactor_kind.rs`
home of `RefactorKind`. Dependents: [#595](https://github.com/uppin/tddy-coder/pull/595)
`repoint-facade` follows it on the line and consumes nothing from it.
