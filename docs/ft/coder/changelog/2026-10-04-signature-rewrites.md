# 2026-10-04 — Signature and call-site operations for restructure plans

`#live-plan` 15/15, PR [#567](https://github.com/uppin/tddy-coder/pull/567). Feature:
[Rust code restructuring](../rust-code-restructuring.md#signature-and-call-site-operations).

A plan can retype a parameter, add or reorder parameters, change a return type, and repair each caller
with ordinary call-site operations: `change_param_type`, `add_param`, `reorder_params`,
`change_return_type`, `add_call_arg`, `remove_call_arg`, `change_call_arg` and `reorder_call_args`.
Put in one transactional group, the declaration and its callers compile only at the group's end; a group
missing a caller is rolled back and names that caller's error.

- `type` and `expr` carry one Rust type or one expression, parsed and refused as malformed otherwise (an
  `expr` may hold no statement); `text`, `code` and `content` are still refused.
- A call-site operation is anchored on one call expression; a range that is not one call is refused.
- `change_return_type` takes a `type`, or a `variant` (`wrap_result`, `wrap_option`, `unwrap`) that uses
  rust-analyzer's assist.
- A parameter is renamed with `rename_symbol` and a range anchor on its name.

Acceptance criteria: each was met by a passing test in `tests/signature_rewrites_acceptance.rs` and the
plan unit tests. Package entry:
[2026-10-04-signature-rewrites.md](../../../../packages/tddy-code-restructuring/docs/changesets/2026-10-04-signature-rewrites.md).
