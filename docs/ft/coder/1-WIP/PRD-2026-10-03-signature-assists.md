# Signature operations that rewrite their own callers - PRD

**Date**: 2026-10-03
**PRD Type**: Enhancement

## Affected Features

- **Primary**: [Rust code restructuring](../rust-code-restructuring.md) — two new operations.

## Summary

Adds the two signature changes rust-analyzer can make *together with every caller*:
`remove_unused_param` (assist `remove_unused_param`) and `convert_tuple_return_to_struct`
(assist `convert_tuple_return_type_to_struct`). Each leaves a compiling tree on its own, so neither
needs a transactional group.

## Background

[`2026-09-24-restructure-has-no-signature-operations.md`](../../../dev/todo/2026-09-24-restructure-has-no-signature-operations.md)
— no operation changes a signature. The caller-breaking ones (`change_return_type`,
`change_param_type`, reorder, add) need a group and call-site ops; they are `signature-rewrites`.

## Proposed Changes

- `remove_unused_param`: anchor = `item` anchor on the function; `name` = the parameter. Resolved
  through `multi_file_assist` like `inline_method`. Refused, naming the use, when the parameter is used
  (rust-analyzer does not offer the assist then).
- `convert_tuple_return_to_struct`: anchor = `item` anchor on the function; `name` = the new struct's
  name (the assist's placeholder renamed through the server).
- Both validated in `parse_op` (field presence), added to `SUPPORTED`, documented in the plan-schema skill reference.

## Acceptance Criteria
- [ ] Removing an unused parameter rewrites the declaration and every call site, and the tree compiles.
- [ ] Removing a used parameter is refused naming the parameter and that it is used; nothing written.
- [ ] Converting `-> (u32, String)` produces the named struct, rewrites destructuring callers, and compiles.
- [ ] A missing `name` is refused as malformed.
