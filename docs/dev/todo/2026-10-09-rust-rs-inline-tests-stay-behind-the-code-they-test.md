# 2026-10-09 — `backends/rust.rs` keeps ~2,560 lines of inline tests for code that now lives in fifteen modules

**Category:** Deferred refactor (test layout)
**Source:** #reshape 17/19 (`feature/reshape/rust-backend-split`), discovery Exploration 2 (E2.1, E2.7), decision F6

After `rust-backend-split`, `packages/tddy-code-restructuring/src/backends/rust.rs` has about 425 production lines and
still carries its whole inline `#[cfg(test)] mod tests` (`:3009` to the end on master, about 2,557 lines, with a nested
`use super::*` module). It also keeps the 46 `#[cfg(test)] use <child>::<name>;` hub lines (`:511-514`, `:2900-2968` on
master) that exist only so the inline tests' `use super::*` sees names defined in the children. The tests exercise code
that now sits in `transport.rs`, `assists.rs`, `assist_edits.rs`, `extraction.rs`, `lsp_edits.rs`, `handshake.rs` and the
others. The engine re-pointed them by inserting `use` lines inside `mod tests`.

## Why deferred

- Test lines are not production lines, so the 500-line budget is met without moving them.
- No operation splits an **inline** test module. `#reshape` 14 (`tests-follow`) carries sibling `#[cfg(test)] mod x_tests;`
  modules only, and moving test functions by hand would break the stack's engine-only rule.

## What would close it

Either an operation that lifts a run of `#[test]` functions out of an inline `mod tests` into the moved code's module (or
into a sibling `x_tests.rs` that `tests-follow` then carries), or an engine `extract_module --to_file` of the whole test
module into `backends/rust/tests.rs` followed by per-group moves. Afterwards the `#[cfg(test)] use` hub lines in `rust.rs`
go away.
