# 2026-10-09: four separate functions strip a Rust visibility prefix

**Category:** Code duplication (restructure engine)
**Source:** #reshape 5/19 (`feature/reshape/move-children`)

## What is duplicated

| Where | Function |
|---|---|
| `packages/tddy-code-restructuring/src/crate_move/manifest_edits.rs` (after #reshape 5; moved from `facade_writer.rs:270`) | the reader behind `declared_module` |
| `packages/tddy-code-restructuring/src/backends/rust/module_text.rs:80` | `strip_visibility` |
| `packages/tddy-code-restructuring/src/verify/statements.rs:179` | `strip_visibility` |
| `packages/tddy-code-restructuring/src/backends/rust.rs:2805` | `visibility_in` |

They differ in edge cases, such as `pub(in path)` with spaces, `pub(self)` and a missing trailing whitespace. Each module's tests pin its own copy.

## What should happen

Use one reader in `crate_move` (`backends` and `verify` already depend on it), with the union of the four test sets. Do it after the crate split stack decides where `crate_move`'s lexical helpers live.

## Why deferred

#reshape 5 needed only the `crate_move` copy fixed. Consolidating touches `backends/rust.rs`, which #reshape 17 is splitting, so it would collide with that node.
