# 2026-10-22 — `fn-sizes-rest`: the function-length gate; five functions cut, ten exempted by name
**Type:** Refactor

- `tests/function_length_budget.rs`: parses every `src/**/*.rs` with `syn` and fails, naming `file:line fn (N lines)`,
  on a function longer than 60 lines (the `fn` line to the closing brace, inclusive), outside `src/backends/`. Measured:
  free functions, `impl` methods and trait methods with a body; not measured: `tests.rs`, `*_tests.rs`, and items under
  `#[cfg(test)]` or `#[cfg(all(test, …))]`. Five tests: the list is empty, the measurement, methods, test code, the
  backend exclusion; a sixth fails when an exempted function no longer runs past 60 lines, so the list only shrinks.
  `Cargo.toml` gains one dev-dependency, `proc-macro2` with `span-locations` (already in the lock), because `syn` spans
  carry line numbers only with it; a scoped build of this package does not unify features with `tddy-code-analysis`.
- Cuts, each an `extract_method` applied by `tddy-tools restructure`, new functions private and in the same file:
  `items_of_module` → `record_child_module` (`crate_move/source_scan/module_items.rs`); `options_for` → `apply_options`,
  `anchors_options` (`restructure_args.rs`); `check_plan` → `rehearse_op` (`runner/entry_points/check_entry_points.rs`);
  `cluster_edits` → `absorb_after_the_loop`, `absorb_across_the_set` (`crate_move/cluster.rs`); `surveyed_in` →
  `survey_reference` (`crate_move.rs`). `stranded_siblings` and `resolve_cluster` were already within 60 on this base.
- The one hand design edit, consented (F2): `apply_held_plan` keeps `PlanRun` whole and holds its loop state in
  `struct HeldLoop` (`runner/entry_points/store_run.rs`, `TODO(reshape-16)`). The function itself stays over 60.
- Verify accounting, accepted by the developer on 2026-10-22 and listed in each cut's commit: the `&mut`-parameter family
  (`x = …` → `*x = …`, `&mut x` → `x`, unchanged autoderef'd access, `&x` → `&*x`) and the lines of an extracted
  match arm.
- **Deferred by developer 2026-10-22**, exempted in `DEFERRED_BY_THE_DEVELOPER`: `refreshed` (the engine spells
  `&mut dyn Trait` as `'static` and rewrites `.clone()` as `.to_owned()`), `sightings` (an outer cursor is not passed
  to the new function), `parse_op` (the guard lifts re-type borrowed fields: 12 compile errors), `apply_held_plan` (the
  tail is refused for comment carry; the loop's phases need outputs), `cluster_edits` and `check_plan` (one further cut
  each lost a `mut`), and `sorted`, `follow_changes`, `member_changes`, `compare_with`, which grew past 60 after the
  plan was written. `store_run.rs` (507) and `crate_move.rs` (514) are in `NOT_YET_SPLIT` of
  `tests/engine_file_budget_shape.rs`.
- Code-issue records: none of this package's ten covers a cut function, so none is closed.
  `complexity-runner-preflight-moves` is re-measured unchanged (6, 8 and 7 parameters).

Docs: [README.md](../../README.md), [Rust code restructuring § Known limitations](../../../../docs/ft/coder/rust-code-restructuring.md#known-limitations).
