# Cross-crate moves widen what the origin still reaches, and say so - PRD

**Date**: 2026-10-09
**PRD Type**: Enhancement
**Stack**: `#reshape` 7/19 (`feature/reshape/move-widen`)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `## Rust operations (v1)` (the
  `move_module_to_crate` / `move_cluster_to_crate` rows gain a visibility rule), `## Path survey` (what counts as reached
  and what the survey line says), `## Known limitations` (what is still not widened).

No other feature document changes. No plan field, flag or wire message is added; the journal already carries the report.

## Summary

A cross-crate move (`move_module_to_crate`, `move_cluster_to_crate`) makes `pub` every declaration in the files it moves
that something staying outside them still reaches — items, fields and inherent methods of reached types, items of inline
modules and `mod` declarations along the path. It also widens every item the moved module's parent re-exports by glob,
and every type the signature of a widened item names. It finds them by declaration position from rust-analyzer, never by
name. It never touches what nothing outside reaches. Every widening is reported: by `apply` (as `move_item` already does)
and, new, by `check --deep`, before anything is written.

## Background

On `#carve` 21/21 every cross-crate milestone ended in a `cargo check` loop. Hand fixes turned `pub(crate)` into `pub`:
1 item in R2, about 35 items in R6, about 140 items and fields in R9, 275 in all. The loops made their own mistakes. A
name-based rewrite widened same-named parameters, and `pub` landed on a trait method (E0449). The engine already knows
the answer. The survey reports "1 item(s) reached from outside: mint_first_admission_token" and then leaves that item
`pub(crate)`. Same-crate moves have widened and reported since `move_item` shipped. The cross-crate moves, where crossing
a crate boundary forces the widening, do neither. `check --deep` also drops the widening report of every operation, so a
plan author learns the cost of a move only after paying for it.

Across a crate boundary the only visibility that compiles is `pub`. "As little as needed" is therefore a question of
**which** declarations get widened, not how far: only what something outside the moved files reaches, and never the rest.

## Proposed Changes

### What's Changing

- **What is widened to `pub`**, when it is written private, `pub(crate)`, `pub(super)` or `pub(in …)`, and is reached by a
  reference in a file the move does not carry and that is not in the destination crate:
  - a module-level item of a moved file (the survey's existing `reached from outside` set);
  - an item of an inline module in a moved file, and the `mod` declarations on its path (`m::inner::X`);
  - a file-backed `mod child;` declaration in a moved file that outside code names;
  - a field of a struct, and a method or associated const of an **inherent** `impl` in a moved file.
- **Glob-visible items.** When the file that declares the moved module re-exports it by glob (`use m::*;` at any
  visibility), every item of the module that is not private counts as reached, with or without a reference. A glob makes
  no promise about which items it carries, and references cannot see a trait that is imported only so its methods can be
  called.
- **Escaping types.** A type that a widened item's signature names, or a widened field's type, is widened too, until
  nothing new is widened. Otherwise a `-D warnings` gate fails on `private_interfaces`.
- **Restricted visibilities are widened, never respelled.** A reached declaration written `pub(super)` or
  `pub(in crate::…)` becomes `pub` like any other. An unreached `pub(in crate::<origin module>)` is left for the crate
  move's own visibility rewrite (`#reshape` 8/19, `feature/reshape/move-grouped-use`), which reads it as a visibility and
  rewrites it for the destination. This node does not touch it.
- **Never touched**: members of `impl Trait for T`, trait items, enum variants (they take no visibility), declarations
  nothing outside reaches, and anything already `pub`. Edits are addressed by the declaration position rust-analyzer
  reports, so a parameter or another item with the same name is never rewritten.
- **Refused**, naming the declaration, with nothing written: a reached declaration whose visibility keyword is on a
  line above its name, which the position cannot address. The same refusal already exists for `move_item`.
- **Reported**: one `visibility:` line per widening, `` `AttachmentState` pub(crate) -> pub ``. A member is written
  `Type::member`. A reason is added where the widening has no reference of its own: ``(through `connection_service.rs`'s
  glob)`` or ``(named by the signature of `prepare`)``. `apply` prints these lines and the journal records them, as it
  does for same-crate moves.
- **`check --deep` prints widenings** for every operation it rehearses (`move_cluster_to_crate` too, which it resolves
  but does not survey). As a side effect, `extract_module` and `move_item` widenings now show in `check --deep` as well.
- **The survey line explains a glob-reached module**: when a module has reached items but no callers to re-point,
  `check --deep` names the parent glob that carries them.

### What's Staying the Same

- Callers, facades, manifests, the cycle refusal and the path survey's rewrites — only visibility keywords in the moved
  files change.
- Same-crate moves (`move_item`, `reparent_module`) and `extract_module` keep their own widening rules.
- `move_test_binary_to_crate` widens nothing: no code can name a test binary.
- No narrowing pass: items hand-widened by earlier stacks stay as they are.

## Impact Analysis

### Technical Impact
- `tddy-code-restructuring` only. The `ModuleReferences` seam gains declaration positions, written visibilities and
  members. A new deciding module goes under `crate_move/`. `resolve_cluster` returns a `Resolution`. `Rehearsed` carries
  the report. The positional visibility edit is lifted to a shared `backends/rust/` helper.
- Cost: one extra `textDocument/references` per field and inherent member in the moved files, on top of the one per item
  paid today. That is measured and stated in the changeset, not bounded by a budget.
- One new live test binary joins the e2e filterset and the `rust-analyzer` test group. The deciding rules are unit-tested
  over the fake reference set, with no server.

### User Impact
- The `cargo check` loop after a cross-crate move shrinks to what is listed under Known limitations. `check --deep` tells
  the plan author how many declarations a move will widen before it runs.
- Output change for existing plans: `check --deep` now prints `visibility:` lines. No breaking change.

## Implementation Plan

1. Reference seam: members, inline-module items, declaration positions and visibilities (fake and Rust backend).
2. Deciding step: reach, glob-visible, escaping types, and the exclusions (library level, no server).
3. Edits merged into the moved files' existing change, and the `Resolution` report.
4. `check --deep`: carry and print the report, and add the glob explanation to the survey line.
5. Thin live binary and its registration.
6. Docs at wrap (feature doc rows, Known limitations, `path-survey.md`).

## Acceptance Criteria

- [ ] a module moved with a facade, whose `pub(crate)` fn, struct, fields and inherent methods the origin still uses,
  compiles after the move with no hand edit, and each widening is reported ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] a nested module re-exported by its parent's `pub(crate) use m::*;` (the 09-25 shape) compiles after the move, and
  every non-private item of it is reported as widened through the glob
- [ ] a declaration that nothing outside reaches keeps its visibility. A `pub(crate)` item used only by a co-moving
  member stays `pub(crate)`
- [ ] a same-named parameter, field or function elsewhere is byte-identical afterwards. A member of `impl Trait for T`
  is never given `pub`
- [ ] a type named in a widened item's signature is widened, and the moved crate passes `cargo clippy -- -D warnings`
- [ ] a reached declaration written `pub(super)` or `pub(in crate::…)` becomes `pub`. An unreached one is
  byte-identical afterwards
- [ ] `check --deep` prints every widening `apply` would make, writes nothing, and explains "0 caller(s)" for a glob-reached
  module
- [ ] the keyword-above-name refusal names the declaration and writes nothing
- [ ] tests pass for `tddy-code-restructuring` (scoped; CI for the rest)

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-move-widen.md`
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-move-widen-initial-discovery.md`
- Todos this closes: [move-to-crate leaves a pub(crate) fn the facade caller needs](../../../dev/todo/2026-10-08-restructure-move-to-crate-leaves-a-pub-crate-fn-the-facade-caller-needs.md),
  [move-to-crate leaves pub(crate) items the origin still uses](../../../dev/todo/2026-10-08-restructure-move-to-crate-leaves-pub-crate-items-the-origin-still-uses.md),
  [nested module's parent glob](../../../dev/todo/2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md)
- Todo this narrows (item 3 only): [defects from the first cross-crate move](../../../dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md)
- Reference, not closed: [narrow the items the carve moves widened to pub](../../../dev/todo/2026-10-08-narrow-the-items-the-carve-moves-widened-to-pub-that-no-other-crate-uses.md)
- Survey design: [path-survey.md](../../../../packages/tddy-code-restructuring/docs/path-survey.md); same-crate
  precedent: [same-crate-moves.md](../../../../packages/tddy-code-restructuring/docs/same-crate-moves.md)
