# 2026-10-09 — no gate measures function nesting depth in `tddy-code-restructuring`

**Category:** Code quality (missing gate)
**Source:** #reshape 19/19 (`fn-sizes-backend`), decision F6, found while closing `complexity-rust-facade-lines.md`

`/analyze-clean-code` grades nesting depth ("max levels of indentation in a function") above 4 as "must refactor"
(`.agents/commands/analyze-clean-code.md:39-43`). The crate gates file length (node 15) and function length (node 16's
`tests/function_length_budget.rs`), and nothing gates nesting.

Two measures disagree on the same function. `facade_lines`'s record reads nesting 5, the indentation measure, which
counts rustfmt's extra indent for a method chain (`.map(`). A brace scan, with the fn body at depth 1, reads 4. By brace
depth, 8 functions of the crate are past 4 on 2026-10-09, two under `backends/`: `rust.rs:196 client_capabilities` (a
`json!` literal, which is data) and `rust/item_move/canonical_paths.rs:120 collected`.

## Why deferred

A gate needs one agreed measure first. The candidates are indentation levels, brace depth, or a `syn` walk of nested
blocks, closures and arms, each with a stated rule for data literals such as `json!`. It also needs the count under that
measure, before a cap is set. `#reshape` 19 closes the one record by cutting the function and re-measuring with the
record's own measure, which needs no gate.

## What would make it worth doing

A second nesting record, or the 41–60 length work
(`2026-10-09-functions-of-41-to-60-lines-in-tddy-code-restructuring.md`) starting, since the same cuts lower both.
