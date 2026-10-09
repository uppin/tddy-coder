# `tddy-code-restructuring`: no function outside the backend runs past 60 lines - PRD

**Date**: 2026-10-09
**PRD Type**: Refactor (behaviour-preserving) with a new test gate

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — no section changes behaviour. The
  `## Known limitations` bullets on `extract_method` gain nothing new; this node is their first real consumer after
  `#reshape` 4.

No other feature document changes. No operation, plan field, flag, wire message or output line changes.

## Summary

The nine production functions of `tddy-code-restructuring` that run past 60 lines outside `src/backends/` are cut to 60
or fewer with the crate's own `extract_method`, driven by `tddy-tools restructure` plans. A new test fails when any
production function outside `backends/` runs past 60 lines, so the crate stays that way. Node 19 extends the same test to
`backends/`.

## Background

`.agents/commands/analyze-clean-code.md` says a function over 60 lines must be refactored. The whole-work discovery
measured 25 such functions in this crate; nine are outside `backends/`: `parse_op` (206), `apply_held_plan` (159),
`check_plan` (115), `sightings` (95), `refreshed` (76), `options_for` (73), `stranded_siblings` (70), `resolve_cluster`
(66) and `items_of_module` (64). Nothing measures function length today: the `/pr-wrap` gate and `check --budget` measure
files. Doing this with the engine, not by hand, is also the test of `#reshape` 4 (`extract-method-clean`): every extraction
should come out with its comments and a lint-clean signature.

## Proposed Changes

### What's Changing

- **Each of the nine functions is 60 lines or fewer** (fn line to closing brace, production code), on the tree as it is
  after nodes 1–15. The list is re-measured at the start of the work, because nodes 3, 5–10, 12 and 13 edit five of these
  functions first.
- **Cuts are `extract_method` operations** in restructure plans under the change's working directory, applied with
  `tddy-tools restructure apply`, one plan per function (several for `parse_op`). New functions stay in the same file,
  private. Every seam takes 7 parameters or fewer, so `clippy::too_many_arguments` stays quiet.
- **`parse_op`** becomes a flat list of guard calls (`refuse_…(&op)?;`), one per rule theme, in today's order, so the
  first refusal a line meets is unchanged. Each guard is an `extract_method` of the rules' `if … { return Err(…) }`
  blocks, which `#reshape` 4 now lifts into a function returning `Result<()>` that the caller calls with `?` (its rule
  12). Checked on today's text: every `return` in the seven ranges is `return Err(…)` and none declares a binding read
  later, so none is refused for mixing returns (node 4's F8) or for outputs (F10). Each guard keeps its rules' comments.
  (Decision F1.)
- **`refreshed`'s `Item` arm** is cut with `#reshape` 4's tail-position rule (its rule 13): the range from the arm's
  `if` to its tail `Ok(anchor)` ends in tail position, so its two `return Ok(anchor)` stay as they are and the call is
  the arm's value.
- **`apply_held_plan`** gets one hand design edit first, with consent: the loop's own state is gathered in one struct and
  `PlanRun` is kept whole, marked `TODO(reshape-16)` and recorded in a todo entry. Its phases are then cut by the engine.
  (Decision F2.)
- **A new test**, `packages/tddy-code-restructuring/tests/function_length_budget.rs`, parses every production `.rs` under
  `src/` with `syn` and fails, naming each `file:line fn (N lines)`, when a function outside `src/backends/` runs past 60
  lines. Test code (`*_tests.rs`, `tests.rs`, `#[cfg(test)]` items) is not measured.
- **Hand edits** are allowed only to make the tree build after an engine operation, plus the one consented design edit
  of F2. Each one is recorded as a todo entry
  naming the operation and the edit. When the engine refuses a seam, the work stops and the developer is asked.

### What's Staying the Same

- Behaviour: every existing test of the crate passes unchanged; no message, refusal or output line changes.
- File layout: no module is added, moved or renamed; no `use` edge between modules is added.
- `src/backends/` (node 19), the 71 functions at 41–60 lines (deferred, node 19 writes their todo), file sizes (node 15).
- The engine itself: this node changes no restructure behaviour. A seam the engine cannot cut is a decision, not a fix.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring` only: `src/plan/codec.rs`, `src/runner/entry_points/{store_run,check_entry_points}.rs`,
  `src/crate_move/source_scan/{sighting_walk,module_items}.rs`, `src/plan_store/refresh.rs`, `src/restructure_args.rs`,
  `src/crate_move/cluster.rs`, `src/crate_move/cluster/stranded.rs`; one new test file; one dev-dependency line
  (`proc-macro2` with `span-locations`, already in `Cargo.lock`; consented, F4).
- No live rust-analyzer test is added: the gate reads source text. The restructure runs themselves need the warm index
  (`./run-index-daemon`), not CI.

### User Impact

- None for plan authors. For maintainers: a function past 60 lines outside `backends/` fails the crate's tests.

## Implementation Plan

1. The gate test, red on the current tree, naming the nine functions.
2. Re-measure on the post-node-15 base; re-read each seam; write one plan per function; `check --deep` each.
3. Apply in increasing risk: `items_of_module`, `options_for`, `refreshed`, `stranded_siblings`, `resolve_cluster`,
   `check_plan`, `sightings`, `parse_op` (guard lifts, bottom-up), `apply_held_plan` (the hand state struct, then the
   engine).
4. After each apply: compile gate, tidy, `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`, the scoped
   tests; a todo per hand fix.
5. The gate test green; docs at wrap.

## Acceptance Criteria

- [ ] `function_length_budget.rs` lists no production function outside `src/backends/` past 60 lines
- [ ] the gate measures from the `fn` line to the closing brace, skips test code, and reports `file:line fn (N lines)`
- [ ] every cut was made by `tddy-tools restructure apply`; every hand edit after one has a todo entry, and the one
  design edit (F2) carries a `TODO(reshape-16)` marker and its own todo entry
- [ ] `parse_op` refuses every line it refused before with the same message, first refusal first
- [ ] no module, `mod` declaration or cross-module `use` is added
- [ ] the extracted functions keep the comments their ranges held and pass `cargo clippy -p tddy-code-restructuring
  --all-targets -- -D warnings`
- [ ] tests pass for `tddy-code-restructuring` (scoped; CI for the rest)

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-fn-sizes-rest.md`
- Initial discovery: `docs/dev/1-WIP/2026-10-09-reshape-fn-sizes-rest-initial-discovery.md` (Exploration 3 of the whole
  work and this node's Exploration 2)
- Thresholds: `.agents/commands/analyze-clean-code.md` § Function length
- Skill: `.agents/skills/code-restructuring/` (plan schema, `extract_method`)

## Decisions (developer, 2026-10-09)

- **F1 — `parse_op`: flat guard functions (decided).** The guard lift that `#reshape` 4 now delivers turns each run of
  `if … { return Err(…) }` rules into `fn refuse_…(op: &RefactorOp) -> Result<()>`, called as `refuse_…(&op)?;`. That
  reads as the flat list the existing `plan/codec/*.rs` rule files already form, so it replaces the four-stage tail chain.
  If the guard lift refuses a range, the work stops and asks; the tail chain is the alternative to offer then. Table dispatch is rejected: the rules span kinds,
  so `parse_op` is not an op-name match.
- **F2 — `apply_held_plan`: one consented hand edit.** Gather the loop's own state (`overlay`, `done`, `group`,
  `stopped_early`) in one struct and keep `PlanRun` whole, then cut the phases with `extract_method`. The edit is marked
  `TODO(reshape-16)` and recorded in `docs/dev/todo/2026-10-09-apply-held-plan-run-state-was-grouped-by-hand.md`.
- **F3 — `sightings`: decided at the recommendation.** Try the closure seams under `check --deep`. If they are refused,
  stop and ask whether to replace the closure's two calls by hand or exempt the function at about 74 lines.
- **Dependency risk raised with `#reshape` 4.** Its return-type respelling (rule 14) covers guard lifts and tail
  ranges only. A plain extraction holding `?` gets `Result<X, RestructureError>`, which does not compile in a file that
  binds the crate's one-argument `Result`, and all nine files do. Asked: apply it to every extracted function. If that
  is not taken, each such seam here needs a one-line hand fix after its apply, with a todo each.
- **F4 — the gate's dependency: consented.** A `proc-macro2` dev-dependency with `span-locations`.
- **F5 — exemptions: decided at the recommendation.** None. If F3 ends in an exemption, the gate carries a closed list
  that fails when an exempt function drops to 60 lines or fewer.
