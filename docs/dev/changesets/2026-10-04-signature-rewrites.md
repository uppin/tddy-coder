# 2026-10-04 — Signature and call-site operations for transactional groups

**Type:** Feature

`#live-plan` 15/15, PR [#567](https://github.com/uppin/tddy-coder/pull/567),
`feature/live-plan/signature-rewrites`, base `feature/live-plan/session-restructure-tools`. Product entry:
[2026-10-04-signature-rewrites.md](../../ft/coder/changelog/2026-10-04-signature-rewrites.md).

`restructure` plans can change a function's signature and repair each caller as its own operation, and a
transactional group makes the pair one unit that must compile at its end: `change_param_type`,
`add_param`, `reorder_params`, `change_return_type` (a `type`, or a `variant` of `wrap_result`,
`wrap_option` or `unwrap` through the rust-analyzer assist), and the call-site operations `add_call_arg`,
`remove_call_arg`, `change_call_arg` and `reorder_call_args`. The `type` and `expr` fields carry Rust
syntax, each parsed with `syn` as exactly one `Type` or `Expr` and refused otherwise; `text`, `code` and
`content` stay refused. The operations edit the declaration or the one call they are anchored on and never
fan out.

**Backlog resolved:** `2026-09-24-restructure-has-no-signature-operations` — the four parameter and
return-type operations are delivered as per-call-site operations by design, and a parameter rename through a
range anchor is documented in `plan-schema.md`. The entry is deleted.

**Deferred, with the developer's consent:** `backends/rust.rs` stays about 125 production lines over its
pre-PR size (2,706 to 2,831): the engine refused to move the impl members, because rust-analyzer cannot put
a `mod` inside an `impl` body. Tracked in the package's `oversized-file-backends-rust` code-issue record,
whose seam is `2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass` § 1. Files this PR
brought under the 500-production-line budget by engine moves: `signature_rewrites.rs` (502 to 170, with
children `declaration.rs` and `call_site.rs`) and `plan/codec.rs` (563 to 437, with child
`codec/signature_fields.rs`).

| Package | Entry |
|---|---|
| `tddy-code-restructuring` | [signature-rewrites](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-04-signature-rewrites.md) — eight operations, `type`/`expr`/`order` fields, `syn` |
