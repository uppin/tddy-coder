# 2026-09-17 — Nested cross-crate moves, facade cycle attribution, check preflight

**Type:** Bug Fix

Stack root [#488](https://github.com/uppin/tddy-coder/pull/488) (`#carve` 1/10).

**Nested `move_module_to_crate`.** `ModuleHome` and `module_home` locate a module's declaring parent
(`<parent>.rs` or `<parent>/mod.rs`) instead of requiring `<crate>/src/<module>.rs`. Acceptance
tests in `nested_module_move_acceptance.rs`.

**Facade cycle attribution.** `defining_crate` resolves origin-named paths through `pub use`
re-exports to the crate that defines the item, so back-compat facades no longer read as origin
dependencies. Genuine origin-defined dependencies are still refused. `facade_cycle_acceptance.rs`.

**Check preflight.** `restructure check` runs `move_preconditions` (same gate as apply before
rust-analyzer) and reports findings with operation index. `check_precondition_parity.rs`; wired
through `runner/entry_points.rs` after the master runner split.

**Public surface:** `module_home`, `defining_crate`, `unrunnable_moves`, `ModuleHome` re-exported
from `lib.rs`.

287 → 318 unit tests plus acceptance suites; scoped gate `./test -p tddy-code-restructuring`.

**Deferred:** mechanical `extract_module` splits of `crate_move.rs` (phases A/C in the planning
changeset); live monorepo apply smoke for refusals (acceptance fixtures cover behavior); AC8
`--indexing-budget` superseded by cancellation on master.

Feature limitations updated in
[rust-code-restructuring.md](../../../../docs/ft/coder/rust-code-restructuring.md#known-limitations).
