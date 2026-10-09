# 2026-10-09 — Five crates' shape tests count production lines with their own heuristics

**Category:** Defect (measurement drift)
**Source:** #reshape 15/19 (`oversized-files`)

#reshape 15 gave the repository one production-line count. It leaves out each `#[cfg(test)]` /
`#[cfg(all(test, …))]` item wherever it sits. It is exposed as
`tddy_code_restructuring::production_lines_of_file` and `tddy-tools restructure lines`, and `/pr-wrap` step 3.5 uses it.
These shape tests still count their own way, each by "lines before some `#[cfg(test)]`":

- `packages/tddy-daemon-rpc/tests/rpc_handlers_shape.rs:63` (`production_lines_of`, the mod-aware cut, which stops at an
  out-of-line `#[cfg(test)] mod x;`)
- `packages/tddy-core/tests/core_facade_shape.rs:107`
- `packages/tddy-workflow-recipes/tests/module_shape.rs:17`
- `packages/tddy-daemon-kernel/tests/telegram_extraction_shape.rs:104`
- `packages/tddy-presenter/tests/presenter_split_shape.rs:116`

The `oversized-file-*.md` code-issue records across the repo were also taken with the old `/pr-wrap` `awk`, which stops
at the first `cfg(…test…)` of any kind. Some under-read: `connection_service.rs` read 43 for about 1,570.

## Why deferred

Each budget in those tests (10k per crate, 800 / 500 / 400 per module) was set against its own count. Switching the
count can turn a green shape test red in a crate #reshape does not touch, which is a decision for each owning stack.

## What would close it

Each test calls one shared counter. A `dev-dependency` on `tddy-code-restructuring` would do, or the counter could move
to a small test-support crate. Each crate's numbers are re-taken, and every code-issue record is re-measured with
`tddy-tools restructure lines` when it is next touched.
