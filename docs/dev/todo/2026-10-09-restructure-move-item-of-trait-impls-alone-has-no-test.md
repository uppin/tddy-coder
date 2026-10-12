# 2026-10-09 — `move_item` of a run made only of trait impls has no test

**Category:** Test gap (engine)
**Source:** #reshape 17/19 (`feature/reshape/rust-backend-split`), discovery Exploration 2 (E2.7), plan K line K2

`rust-backend-split` moves `impl Drop for RustBackend`, `impl LanguageBackend for RustBackend` and
`impl ModuleReferences for RustBackend` with one `move_item` line anchored on
`['<RustBackend as Drop>', '<RustBackend as LanguageBackend>', '<RustBackend as ModuleReferences>']`.
`packages/tddy-code-restructuring/src/backends/rust/item_move/outline.rs:63-89` accepts such a run: every symbol is an
`impl` block, so `Run.items` is empty (an impl has no identifier name). That leaves the reference survey, the facade and
the widening with nothing to work on, and no suite covers it. `grep " as [A-Z]…>" tests/` finds only the anchor and
plan-line suites. Node 13's discovery assumed it works ("`move_item` of the block lines already").

## Why deferred

A restructure node adds no behaviour tests. The node's own `check --deep` of plan K is the probe, and a refusal stops and
asks the developer.

## What would close it

A live case in `tests/move_item_into_an_existing_module_acceptance.rs` (or a new suite registered in
`.config/rust-e2e.filterset` and the `rust-analyzer` group): a trait impl whose body calls a private helper of the origin
and reads a private field of the type moves to a sibling module. It must compile with its tests and lint clean. Also a
case where the run reaches a child module of the origin through a relative path (`module_text::…`).
