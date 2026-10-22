# 2026-10-22 — `oversized-files`: production lines leave out test-only items; `restructure lines`
**Type:** Fix

- `crate_move/source_scan/test_only.rs`: `test_only_spans(text)` — the byte span of every item marked by
  `cfg_test_attribute` (`#[cfg(test)]`, `#[cfg(all(test, …))]`), at any depth, from its first outer doc comment or
  attribute through its `;` or the `}` closing its first `{`, read over the masked token stream; nested spans merged.
- `runner/budget.rs`: `production_lines` counts the lines not wholly inside such a span (was: lines before the first
  `#[cfg(test)]` opening a `mod`). The old private `lines_of_file` is `pub fn production_lines_of_file(path, text)`,
  re-exported from `runner` and the crate root. The existing pin
  `does_not_end_early_at_a_test_only_import_above_the_module` now expects 2: a `#[cfg(test)] use` is test-only
  (developer decision, acceptance 2).
- `restructure lines <file>...`: `RestructureCommand::Lines` → `Command::Lines` → `entry_points::lines_entry_point::lines`
  → `Outcome::Measured(Vec<FileLines>)` → `console::file_lines` (`<lines>\t<path>`). `needs_lsp_client` is `false`;
  `tddy-tools`' `answered_without_an_index` answers it in process. An unreadable or non-UTF-8 path is a
  `MalformedPlan` refusal naming it. `tddy-index-daemon` is untouched (its CLI has its own enum).
- Shape tests: `tests/engine_file_budget_shape.rs` holds every `src/**/*.rs` to 500 production lines except
  `NOT_YET_SPLIT` (`backends/rust.rs` and the five deferred files); `tests/engine_module_edges_shape.rs` pins the
  must-not edges of the planned splits, its six tests `#[ignore]`d until those splits land.
- **Deferred by developer 2026-10-21:** the five splits (`crate_move/test_binary.rs` 967, `crate_move/test_modules.rs`
  664, `verify/retarget.rs` 631, `runner/tidy.rs` 542, `backends/rust/item_move/assemble.rs` 521), with their planned
  seams, in `docs/dev/todo/2026-10-21-reshape-oversized-files-splits-deferred.md`.
- **Not done:** `/pr-wrap` step 3.5 still measures with its `awk` cut; switching it to `tddy-tools restructure lines`
  is left in `docs/dev/todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md`.

Docs: [README.md](../../README.md), [Rust code restructuring](../../../../docs/ft/coder/rust-code-restructuring.md).
