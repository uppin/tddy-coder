# Signature and call-site operations for transactional groups - PRD

**Date**: 2026-10-03
**PRD Type**: Enhancement

## Affected Features

- **Primary**: [Rust code restructuring](../rust-code-restructuring.md) — signature-changing and call-site operations; `type` and `expr` fields.

## Summary

Changing a parameter's type, adding or reordering parameters, or changing a return type breaks every
caller. With transactional groups, a plan can make the change and then repair each caller with
**ordinary call-site operations**, and only the group's end must compile. This PRD adds both halves:

| Operation | Anchor | Fields |
|---|---|---|
| `change_param_type` | item (the function) | `name` (parameter), `type` |
| `add_param` | item (the function) | `name`, `type`, `variant` = position (`first`/`last`/`after:<param>`) |
| `reorder_params` | item (the function) | `order` (parameter names) |
| `change_return_type` | item (the function) | `type` (or `variant` = `wrap_result`/`wrap_option`/`unwrap`, assist-backed) |
| `add_call_arg` | item + relative range on one call expression | `expr`, `variant` = position |
| `remove_call_arg` | same | `variant` = position |
| `change_call_arg` | same | `variant` = position, `expr` |
| `reorder_call_args` | same | `order` (argument positions) |

The signature operations edit only the declaration (and, for `reorder_params`, nothing else); each
call site is its own operation — no fan-out.

## Background

[`2026-09-24-restructure-has-no-signature-operations.md`](../../../dev/todo/2026-09-24-restructure-has-no-signature-operations.md)
(the caller-breaking half; the caller-rewriting assists are `signature-assists`'). Plans refuse code
text (`text`/`code`/`content`).

## ⚠ Decision this PRD asks for

`type` and `expr` carry **Rust syntax** in a plan, which relaxes "plans hold intents only". Each is
parsed with `syn` as exactly one `Type` / `Expr` and refused otherwise; `text`/`code`/`content` stay
refused, and no field may carry statements or items. `syn` 2 is already a workspace dependency
(`tddy-code-analysis`); this adds it to `tddy-code-restructuring`.

## Acceptance Criteria
- [ ] A group `change_param_type` + one `change_call_arg` per caller applies and compiles at the group's end.
- [ ] The same group missing one caller is rolled back, naming the caller's error.
- [ ] `reorder_params` + one `reorder_call_args` per caller compiles at the group's end.
- [ ] `change_return_type` with `variant: wrap_result` uses the assist; with `type` edits the declaration.
- [ ] A `type` that is not one Rust type, or an `expr` that is not one expression, is refused as malformed.
- [ ] A call-site op whose range is not a call expression is refused.
