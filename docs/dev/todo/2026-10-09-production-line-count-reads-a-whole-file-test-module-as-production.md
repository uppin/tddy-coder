# 2026-10-09 — The production-line count reads an extracted test module file as production

**Category:** Defect (minor)
**Source:** #reshape 15/19 (`oversized-files`, decision F5)

`tddy-tools restructure lines` and `check --budget` count one file's text. A file that is wholly a test module, declared
out of line by its parent (`#[cfg(test)] mod wide_facade_tests;` → `wide_facade_tests.rs`), carries no `cfg(test)`
marker of its own. So every line of it counts as production. `/pr-wrap`'s path exclusions do not catch it either: it
lives under `src/`, not `tests/`.

## Why deferred

No such file in `tddy-code-restructuring` is over 400 lines (the largest, `runner/tidy/wide_facade_tests.rs`, is 257).
Counting it as 0 means resolving the parent's `mod` declaration (`#[path]`, `mod.rs` and inline parents included), which
is module resolution, not a text count.

## What would close it

Given a file path, the count finds the declaring parent (the `module_files` rules the engine already has) and returns 0
when the declaration is marked `cfg(test)`. Pin it with a tempdir crate holding a 600-line `x_tests.rs`.
