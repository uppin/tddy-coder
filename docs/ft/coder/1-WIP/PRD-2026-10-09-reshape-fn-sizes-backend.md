# `tddy-code-restructuring`: no backend function runs past 60 lines - PRD

**Date**: 2026-10-09
**PRD Type**: Refactor (behaviour-preserving) extending an existing test gate

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — no section changes behaviour. This node
  is the second consumer of `#reshape` 4's `extract_method` clean-up, after `#reshape` 16.

No other feature document changes. No operation, plan field, flag, wire message or output line changes.

## Summary

The sixteen production functions under `tddy-code-restructuring`'s `src/backends/` that run past 60 lines are cut to 60
or fewer with the crate's own `extract_method`, driven by `tddy-tools restructure` plans. The function-length gate that
`#reshape` 16 adds stops excluding `backends/`, so the whole crate stays under the cap. The open code issue on
`facade_lines` (nesting depth 5) is closed by the same kind of cut. The 71 functions at 41–60 lines are deferred to a
backlog entry.

## Background

`.agents/commands/analyze-clean-code.md` says a function over 60 lines must be refactored. The whole-work discovery
measured 25 in this crate; sixteen are under `backends/`, not the seventeen the stack plan named:

| Function | Lines | Kind |
|---|---|---|
| `rust.rs` `resolve_opening` | 163 | op dispatch, 13 value returns |
| `rust.rs` `assisted_edit` | 124 | assist pipeline |
| `rust.rs` `assist_for` | 93 | data table, 8 arms |
| `rust/item_move/sites.rs` `edits_for_file` | 91 | two loops over shared state and two closures |
| `rust.rs` `start` | 87 | server spawn and handshake |
| `rust/item_move/assemble.rs` `assemble` | 82 | edit assembly |
| `rust/item_move.rs` `move_items` | 73 | `move_item` resolution |
| `rust/item_move/assemble.rs` `visibilities` | 71 | two loops |
| `rust.rs` `offered_assist` | 71 | polling loop |
| `rust/retarget_impl.rs` `retarget_impl` | 70 | `retarget_impl` resolution |
| `rust/visibility.rs` `restore_visibility` | 66 | one loop, heavily commented |
| `rust/item_move/sites.rs` `rewrite_statement` | 66 | `use` rewrite |
| `rust.rs` `check` | 66 | per-op static findings |
| `rust/import_text.rs` `choose_import` | 65 | three tiers, heavily commented |
| `rust.rs` `request` | 64 | LSP request, bridged or stdio |
| `rust/imports.rs` `next_import` | 62 | per-name loop |

`#reshape` 17 (`rust-backend-split`) first moves the seven `rust.rs` functions into modules under `backends/rust/`, and
`#reshape` 18 (`backend-session`) turns `RustBackend`'s operations into free functions over a session handle. This node
cuts what those leave.

## Proposed Changes

### What's Changing

- **Each of the sixteen functions is 60 lines or fewer** (`fn` line to closing brace, production code), on the tree as it
  is after nodes 1–18. The list is re-measured at the start of the work. A function that has crossed 60 by then (five
  sit at exactly 60 today) is cut under the same rules, and the node that grew it is reported.
- **Cuts are `extract_method` operations** in restructure plans, applied with `tddy-tools restructure apply`, one plan per
  function (one per stage for `resolve_opening`). New functions stay in the same file and the same `impl` block, private.
  Every new function takes 7 parameters or fewer, the receiver counted, and 5 or fewer where the code allows.
- **`resolve_opening`** is cut as a **tail chain**: four ranges, bottom-up, each running to the function's tail, so its
  `return`s keep their meaning (today's exception). It keeps the unsupported-op guard, the file read, a text-refusal call
  and one call. (Decision F3.)
- **`assist_for`** keeps its `match`. Each arm's `Assist { … }` literal becomes a zero-argument function named for the
  assist. (Decision F4.)
- **`edits_for_file`** gets one hand design edit first, consented (F2): its two closures and the values derived from its
  parameters become one private struct with two methods. The edit is marked `TODO(reshape-19)` and recorded in a todo
  entry. Its two loops are then cut by the engine. (Decision F2.)
- **`facade_lines`** (the claimed record, 58 lines and nesting 5) loses its `named` arm body, then the tier closure body,
  to two functions; each is at nesting 3 or less under the record's own measure. The record is deleted at wrap with the
  final measurement. (Decision F6.)
- **The gate** in `packages/tddy-code-restructuring/tests/function_length_budget.rs` measures all of `src/`, including
  `src/backends/`; node 16's exclusion and the test pinning it are removed.
- **The deferral.** A backlog entry lists the 71 functions at 41–60 lines (45 under `backends/`, 26 outside, measured
  2026-10-09) with why they are deferred.
- **Hand edits** are allowed only to make the tree build after an engine operation, plus the one consented design edit
  of F2. Each is recorded as a todo entry naming the plan, the operation and the edit. When the engine refuses a seam,
  the work stops and the developer is asked.

### What's Staying the Same

- Behaviour: every existing test of the crate passes unchanged; no message, refusal, progress line or output changes.
- File layout: no file, module or `mod` declaration is added, moved or renamed, and no `use` line is added.
- The engine: this node changes no restructure behaviour. A seam the engine cannot cut is a decision, not a fix.
- Functions outside `backends/` (node 16), file sizes (nodes 15 and 17), and the 41–60 band (deferred).

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring` only: the files that hold the sixteen functions after node 17 (its new
  `backends/rust/{language_backend,extraction,assists,transport}.rs` for the seven `rust.rs` functions, bodies unchanged;
  `rust/item_move/{sites,assemble}.rs`, `rust/item_move.rs`, `rust/retarget_impl.rs`, `rust/visibility.rs`,
  `rust/import_text.rs`, `rust/imports.rs`), `rust/facade.rs`, and the gate test. No new dependency: node 16 adds the
  gate's `proc-macro2` line.
- Each extraction adds a signature and braces (≈ +200 lines in all). Every touched file stays within node 15's 500-line
  file cap.
- Node 18 (same wave) detaches six of the functions into free functions over `session: &mut RustBackend`
  (`resolve_opening`, `assisted_edit`, `offered_assist`, `move_items`, `retarget_impl`, `next_import`); those are cut after
  its conversion is on this branch's base. `start`, `request` (session module `transport.rs`) and `check` (a
  `LanguageBackend` member) stay methods and are cut early with the free functions. (Decision F5.)
- No live rust-analyzer test is added. The restructure runs need the warm index (`./run-index-daemon`) and a
  `tddy-tools` built before the first cut.

### User Impact

- None for plan authors. For maintainers: a function past 60 lines anywhere in the crate fails the crate's tests.

## Implementation Plan

1. The gate covers `backends/`, red on the base, naming the backend functions.
2. Re-measure on the post-node-18 base; re-read each seam; write one plan per function; `check --deep` each.
3. Free functions, increasing risk: `facade_lines`, `restore_visibility`, `rewrite_statement`, `visibilities`,
   `assemble`, `choose_import`, `assist_for`, then `edits_for_file` (the hand struct, then the engine).
4. `request`, `start`, `check` (methods in node 18's end state), then, after node 18's conversion: `next_import`,
   `retarget_impl`, `move_items`, `offered_assist`, `assisted_edit`, `resolve_opening` (four tail stages, bottom-up).
5. After each apply: compile gate, tidy, `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`, the
   scoped tests, and a todo per hand fix.
6. The gate is green and the deferral entry is written; docs are updated at wrap.

## Acceptance Criteria

- [ ] `function_length_budget.rs` lists no production function in `src/` past 60 lines, `src/backends/` included
- [ ] a 61-line function under `src/backends/` in a fixture tree is reported as `file:line fn (61 lines)`; a 60-line one
  is not
- [ ] every cut was made by `tddy-tools restructure apply`; every hand edit after one has a todo entry, and the F2 design
  edit carries a `TODO(reshape-19)` marker and its own todo entry
- [ ] `resolve_opening` returns the same `Resolution` or refusal for every operation kind, in the same order of checks
- [ ] `facade_lines` and the functions cut from it measure nesting 4 or less by the record's indentation measure; the
  record is deleted at wrap with that measurement in its history
- [ ] no file, `mod` declaration or `use` line is added; every new function is private
- [ ] the extracted functions keep the comments their ranges held and pass `cargo clippy -p tddy-code-restructuring
  --all-targets -- -D warnings`
- [ ] a backlog entry lists the 41–60-line functions with their count
- [ ] tests pass for `tddy-code-restructuring` (scoped; CI for the rest)

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-fn-sizes-backend.md`
- Initial discovery: `docs/dev/1-WIP/2026-10-09-reshape-fn-sizes-backend-initial-discovery.md` (Exploration 3 of the whole
  work and this node's Exploration 2)
- Code issue: `packages/tddy-code-restructuring/docs/code-issues/complexity-rust-facade-lines.md`
- Thresholds: `.agents/commands/analyze-clean-code.md` § Function length, § Nesting depth, § Parameter count
- Skill: `.agents/skills/code-restructuring/` (plan schema, `extract_method`)

## Decisions (decided by the developer, 2026-10-09)

The PRD was approved with every recommendation below.

- **F1 — `?` under the crate's one-argument `Result` — resolved by `#reshape` 4.** rust-analyzer writes
  `Result<X, RestructureError>` for a range propagating with `?`, which is `E0107` under `crate::Result<T>`. Node 4's rule
  14 respells the new function's return type to the caller's one-argument spelling for **every** `extract_method`
  (`extracted_fn/return_type.rs`, its tests 33–35), and accepts a guard run that also holds `?`. A seam that still comes out
  mis-spelled is a node 4 defect: stop, report, todo.
- **F2 — `edits_for_file`: one consented hand edit.** A private `FileSites` struct (`context`, `path`, `text`, `masked`,
  `base`, `qualifier`) whose `in_region`/`in_destination` methods replace the two closures, marked `TODO(reshape-19)` and
  recorded in `docs/dev/todo/2026-10-09-edits-for-file-site-state-was-grouped-by-hand.md`; then two engine cuts (statement
  loop, site tail), about 30 lines.
- **F3 — `resolve_opening`: the engine-only tail chain** (four stages, 32–46 lines each, 6 parameters counting the
  session), read as `authored_resolution` → `crate_move_resolution` → `text_resolution` → `assisted_resolution`. No hand
  `match` rewrite.
- **F4 — `assist_for`: all eight arms become zero-argument functions**, 17 lines left.
- **F5 — ordering with node 18.** First: the free functions, `facade_lines`, and `start`, `request`, `check`, which node 18
  keeps as methods (its F4: session-module members and trait members stay methods; a helper extracted from `start` or
  `request` stays legal in `transport.rs`). After node 18's conversion is on the base, re-anchored: the six it detaches
  (`resolve_opening`, `assisted_edit`, `offered_assist`, `move_items`, `retarget_impl`, `next_import`); an extraction
  there writes a free helper taking `session`.
- **F6 — closing `complexity-rust-facade-lines.md`:** cut the `named` arm and the tier closure, re-measure with the record's
  own indentation measure, delete the record at wrap. No nesting gate here; the new todo
  `2026-10-09-no-gate-measures-function-nesting-depth.md` holds it.
- **F7 — parameter cap:** 7 counting the receiver or `session` (hard), 5 preferred. The eleven 6–7-parameter seams are
  accepted; `/analyze-clean-code` will report them at wrap.
- **F8 — functions that cross 60 before this node** are in scope under the same rules; the node that grew one is reported.
- **F9 — tails rust-analyzer may misprint** (`choose_import`'s `Option` `?`; `!`-typed loop tails): `check --deep` first;
  a signature that is not the caller's stops the work for the developer; the non-tail seams are preferred.
  `choose_import`'s fallback is an exemption request.
- **F10 — exemptions:** none. If one is forced, node 16's closed, shrink-only list, each entry with a todo.
