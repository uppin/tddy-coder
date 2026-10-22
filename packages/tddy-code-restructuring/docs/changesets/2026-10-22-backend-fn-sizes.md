# 2026-10-22 — `backend-fn-sizes`: the function-length gate covers `src/backends/`; eight functions cut, six exempted by name
**Type:** Refactor

- `tests/function_length_budget.rs` gains `backend_functions_past_the_cap_under` and four tests beside K18's: no backend
  function past 60 lines outside `DEFERRED_BY_BACKEND_FN_SIZES`; every deferred function still runs past the cap (the list
  only shrinks); the measurement names `file:line fn (N lines)` relative to `src/`; a function of exactly 60 lines is within
  the cap. K18's `LEFT_TO_THE_BACKEND_NODE` exclusion and its pin stay; folding the two gates is left to the sweep node.
- Re-measured at green's base (K11–K15 merged): 14 functions. `visibilities` had fallen to 55; `survey` (79) was new.
- Cuts, 19 `extract_method`s applied by `tddy-tools restructure`, new functions private and in the same file:
  `retarget_impl` → `refuse_moved_fields`; `start` → `spawn_command`, `initialize_server`; `into_destination` →
  `imports_block`; `assemble` → `changed_files`, `absorb_doc_links`, `names_of_items`; `move_items` → `resolution_of`;
  `assist_for` → `extract_method_assist`, `extract_variable_assist`, `extract_module_assist`; `offered_assist` →
  `settled_actions`, `target_range`; `resolve_opening` (161 → 54) → `assisted_resolution`, `assisted_edit_resolution`,
  `signature_or_assisted`, `cluster_or_later`, `fields_or_later` (tail peels); `assisted_edit` → `finish_relocation`.
- The one hand edit, consented by the developer on 2026-10-22: `target_range`'s `let target = ..; target` became its
  `if` expression (clippy `let_and_return`), marked `TODO(reshape-sweep)`.
- Verify accounting accepted by the developer on 2026-10-22 and listed per commit: the `&mut` and `&T` parameter
  families, extracted match-arm lines, and the guard-lift `Ok(())` tail.
- **Deferred by developer 2026-10-22**, exempted in `DEFERRED_BY_BACKEND_FN_SIZES`: `choose_import` (E0261, lifetime
  dropped), `request` (E0502, `&mut self` under a field borrow), `next_import` (E0106), `survey` (tail refused; the other
  cut needs nine parameters), `assisted_edit` (110 lines; E0308 on the rename cut), `edits_for_file` (ten parameters,
  two of them closures). Engine defects: `docs/dev/todo/2026-10-22-extract-method-respells-dyn-lifetime-and-clone-as-to-owned.md`
  §§ 7–13; `edits_for_file`: `docs/dev/todo/2026-10-09-restructure-has-no-operation-to-introduce-a-parameter-struct.md`.
- File budget: `item_move/assemble.rs` 521 → 556 production lines (already in `NOT_YET_SPLIT`; stack-shared, not split,
  recorded in `docs/dev/todo/2026-10-06-restructure-item-move-assemble-past-500.md`); `backends/rust.rs` is owned by its
  own record. `item_move.rs` 374 → 378, `retarget_impl.rs` 244 → 256.
- Code-issue records: none of this package's records covers a cut function; none is closed.

Docs: [README.md](../../README.md), [Rust code restructuring § Known limitations](../../../../docs/ft/coder/rust-code-restructuring.md#known-limitations).
