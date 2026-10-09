# Cross-crate moves carry a module's directory children and keep its `mod` visibility - PRD

**Date**: 2026-10-09
**PRD Type**: Enhancement (plus defect fixes)
**Stack**: `#reshape` 5/19 (`feature/reshape/move-children`)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md). Sections:
  - `## Rust operations (v1)`: the `move_module_to_crate` and `move_cluster_to_crate` rows, which gain what moves with a module and which visibility the facade keeps.
  - `## Path survey`: a path after `..` is read.
  - `## Known limitations`: three entries are rewritten or removed. They are "A nested module moves when its parent is locatable", "`check` without `--deep` cannot predict every apply refusal" (one cause fewer) and "A set that moves together must say so" (children no longer need saying).

No other feature document changes. There is no new operation, plan field, flag or wire message. The plan schema (`.agents/skills/code-restructuring/references/plan-schema.md`) changes only in prose.

## Summary

Today a cross-crate move of a module with a `foo.rs` + `foo/` layout leaves the module's directory children behind. After this change they come along:

- **Children move too.** `move_module_to_crate` and `move_cluster_to_crate` move the module's file **and every file its `mod` declarations lead to**, at the same position under the destination. The plan does not have to name them. The children's own paths are re-pointed, and the crates they name join the destination's manifest.
- **Restricted `mod` lines are read.** A module declared `pub(crate) mod x;`, `pub(super) mod x;` or `pub(in …) mod x;` moves like a `pub mod`. The facade left in its place keeps that narrower visibility.
- **No self facade.** A file that arrives in the destination never gets a facade naming the destination crate.
- **`..` paths are re-pointed.** A body path written after `..` (struct update, range) is re-pointed like any other body path.
- **`check` sees what `apply` sees.** `check` reports every child the move cannot carry, before `apply`, as it does for other seams.

## Background

`#live-plan` 12/15 and `#carve` 21/21 (R3, R4, R7, R8) each fell back to a hand move or a hand edit at the same points:

| Run | What the engine did | Hand fix |
|---|---|---|
| `#live-plan` 12/15 | Moved `index_daemon.rs` without its `index_daemon/` children; `check --deep` and `--dry-run` were green; the compile gate reported `E0583` ×4 | `git mv` of the children |
| `#carve` R3, R4 | Children named in `also` were flattened to the destination root, and the moved parent got `pub use <dest>::child;` pointing at its own crate (`E0432`) | Deleted the line, or rewrote it to `crate::` |
| `#carve` R7 | Nine children named in `also` stayed in the origin; their crates were missing from the manifest | `git mv` of the directory and 11 manifest lines |
| `#carve` R8 | `..crate::connection_service::starting_session_metadata(…)` was not re-pointed (`E0433`) | Re-pointed the path by hand |
| `#carve` R1, R4 | A `pub(crate) mod x;` was refused as "declares no `mod x`" | Widened three declarations to `pub mod`, in pre-move commits |

The repository the stack restructures next is `tddy-code-restructuring` itself. Its `crate_move/`, `runner/`, `plan/`, `plan_store/`, `backends/rust/`, `spawn_record/` and `verify/` are all `foo.rs` + `foo/`, and it uses `pub(crate) mod`. Without this change no engine-driven crate split of it is possible: the second stack, nodes 7/8/9/14/18 of this one.

The walker the move needs already exists and is tested (`crate_move/module_files.rs`). The same-crate `reparent_module` already carries children with it. Only the cross-crate path never called the walker.

## Proposed Changes

### What's Changing

**A moved module carries its directory children.**
- A moved module's files are its own file plus everything `module_files::files_of` finds below it. That includes `mod x;` children in either the `x.rs` or the `x/mod.rs` shape, and children of inline `mod a { mod b; }` blocks.
- Each file moves with `git mv` to the same place relative to the module, under the destination's `src/`. For example `crates/o/src/a/b.rs` → `crates/d/src/a/b.rs`, and for a nested anchor `crates/o/src/x/a.rs` → `crates/d/src/a.rs`, with `crates/o/src/x/a/b.rs` → `crates/d/src/a/b.rs`.
- Every carried file goes through the same path survey and header re-point as the module's own file. The rules are:
  - a `self::` or `super::` path that stays inside the carried tree is left as written;
  - a path out of the tree is re-pointed by the existing rules.
- The crates a carried file names join the destination's `[dependencies]` (or `[dev-dependencies]` under `#[cfg(test)]`).
- Every carried file is included in:
  - the caller survey: references from outside the moved tree to a child's items are re-pointed when `reexport: "none"`;
  - the body precondition (`stays_behind_through_a_body`);
  - the dependency-cycle refusal;
  - the stranded-sibling finding.
- The parent's own `mod child;` lines and any `pub use child::…` inside the moved tree are left **as written**. The destination root declares only the module itself.

**An `also` member that is a directory child of another member is folded into that member.**
- It is accepted, not refused, so existing plans in the R3/R4/R7 shape work.
- It is carried at its nested position: no root declaration, no facade, no flattening.
- A nested member whose parent does **not** move behaves as today. It lands at the destination root under its last segment (`header.rs:47`).

**Refusals, each naming the file, raised before anything is written.** All but the last are static, so plain `check` reports them too; the last needs the reference set, so `check --deep` and `apply` report it:
- a carried `mod x;` that leads to neither `x.rs` nor `x/mod.rs`;
- a carried file that places a child with `#[path]`;
- a target path that already exists in the destination, which would be a merge;
- an operation of the same plan that moves a carried child separately, whether to another destination or after the parent has already left;
- a child declared with a restricted visibility (`pub(crate)`/`pub(super)`/`pub(in …)`) that a file outside the moved tree reaches through the child's own path. After the move that path is private to the destination (`E0603`). The refusal names the child and each referring file. Widening it is not this node's job (decision F3, approved: refuse; follow-up todo `2026-10-09-restructure-crate-move-does-not-widen-a-restricted-child-module.md`).

**Restricted `mod` declarations.**
- `module_declaration` accepts any visibility before `mod`: none, `pub`, `pub(crate)`, `pub(super)`, `pub(self)`, `pub(in <path>)`.
- That fixes the move precondition, the apply-side declaration lookup, re-export following in `module_home`, sorted insertion, and the merge check. The merge check now sees a destination that declares `pub(crate) mod x;`.

**The facade keeps the declaration's visibility.**
- `pub(crate) mod x;` + `reexport: "glob"` → `pub(crate) use dest::x;`.
- `pub(super) mod x;` in a nested parent → `pub(super) use dest::x;`.
- The grouped facade becomes one line per (destination, visibility).
- A later operation extends an earlier line of the same visibility instead of adding a second one.
- A private `mod x;` → private `use dest::x;` (decision F2, approved).

**No self facade.**
- Nothing in the move writes a `use` or re-export that names the destination crate into a file that is itself moving into the destination: no `mod`-line replacement and no `reexport: none` parent-glob rewrite.

**`..` paths.**
- The path reader treats a path after `..` or `..=` as the start of a path, not as a field access.
- `..crate::a::f()` in a struct update and `0..crate::MAX` in a range are therefore surveyed, re-pointed, counted for the manifest and checked by the body precondition.

**Reporting.**
- `apply --dry-run` and `apply` keep printing the same `-> N file(s)` count. Both already count every file the operation writes, and an acceptance test now pins that they agree for a module with children.
- Each crate move adds a note: "`N` file(s) move with `<module>`, with the directory of its children", in the wording `reparent_module` already uses. Readers can then tell moved files from edited ones.

### What's Staying the Same

- **Plan line.** No new field, no new operation.
- **Facades.** `reexport: "named"` and `"outside"` stay refused for a cross-crate move.
- **Lone nested members.** A nested module that moves without its parent still lands at the destination root.
- **Manifest copying.** Crates are still copied from the origin's `[dependencies]`/`[dev-dependencies]`. Target-specific tables (`[target.'cfg(unix)'.dependencies]`, the `libc` case) and attribute-macro-only crates (`async-trait`) are node 9's (`feature/reshape/new-crate`).
- **New crates.** A destination that does not exist is still refused (node 9).
- **Widening.** Items, fields and methods the origin still reaches are not widened (node 7, `feature/reshape/move-widen`), and neither is a restricted child (refused here, see above).
- **Grouped `use` lines.** A group needing different qualifiers is still refused (node 8, `feature/reshape/move-grouped-use`).
- **`pub(in …)` inside moved code.** A `pub(in crate::<origin module>)` restriction written *inside* a moved or carried file is node 8's (it respells it and stops counting it as an edge back). This node reads `pub(in …)` only on the `mod` declaration in the declaring file, which stays in the origin, and mirrors it there in the facade unchanged.
- **The declaration's span.** Which lines a removed `mod` declaration takes with it (doc comments, the attribute refusal) is node 3's (`feature/reshape/tidy-facades`). This node changes only which lines are *recognised* as the declaration.
- **Doc comments.** A doc comment left above a removed `mod` line is node 3's tidy (`feature/reshape/tidy-facades`).
- **Other operations.** Same-crate moves, `extract_module`, `move_test_binary_to_crate`.

## Impact Analysis

### Technical Impact

All changes are in `tddy-code-restructuring`:

- `crate_move/moving.rs`: a member's files and their targets.
- `crate_move/cluster.rs`: carried files flow through rename, header, survey and refusals, and `also` children are folded.
- `crate_move/moving/facade_writer.rs`: no edits to travelling declaring files, and visibility-preserving facades.
- `crate_move.rs` `facade_line` / `facade_lines_for_plan`: a visibility parameter.
- `crate_move/manifest_edits.rs`: visibility-tolerant `module_declaration` / `declared_module_name`.
- `crate_move/preconditions.rs` and `crate_move/cluster/stranded.rs`: read every carried file and add the new refusals.
- `crate_move/source_scan.rs`: the `..` fix.
- `crate_move/module_files.rs`: a shared "files below and where each lands" helper. `module_reparent/relocation.rs` switches to it (decision F4, approved).
- `backends/rust.rs`: the note on the cluster resolution.

Tests:
- A new library-level binary `tests/crate_move_children.rs` with no server.
- Additions to `tests/check_precondition_parity.rs`.
- One compiled case each in `tests/cluster_move_acceptance.rs` and `tests/move_module_to_crate_acceptance.rs`. Both are already in `.config/rust-e2e.filterset` and the `rust-analyzer` nextest group, so no registration change is needed.

Overlap with other nodes:
- **Textual only:** nodes 1–4 and 6–12 also edit `crate_move/*`. Node 8 also widens the stranded-sibling finding in `cluster/stranded.rs`, but from header-only to all paths; this node widens from one file to all carried files.
- **Real edge:** node 14 (`feature/reshape/tests-follow`) extends the "carried files" set with sibling `#[cfg(test)]` modules.

### User Impact

- A `foo.rs` + `foo/` module moves in one plan line, with no `also` list of children, no hand `git mv`, and no hand-written manifest lines for crates the children name.
- `pub(crate) mod` modules move without a pre-move widening commit.
- The facade output changes for a module declared with a restricted or private `mod`: it is no longer silently `pub`.
- Plans in the R3/R4 shape (children in `also`) now nest instead of flattening. Anyone who relied on the flattened layout will see the child under its parent's directory.

## Implementation Plan

1. **Restricted declarations:** `module_declaration` and `declared_module_name` accept any visibility, with unit tests. This unblocks R1/R4-shaped plans on its own.
2. **`..` path reading:** the `continues_a_path` fix, with sighting and survey unit tests.
3. **Carried files:** a shared helper in `module_files`, a member's file set, renames to nested targets, the header pass and manifest over every carried file, and no root declaration for children.
4. **`also` folding** and the **no-self-facade** rule in `left_behind`.
5. **Caller survey, body precondition, cycle refusal and stranded-sibling finding** over carried files, plus the new static refusals. Static `check` parity tests.
6. **Visibility-preserving facades:** grouping per (destination, visibility), and extending the earlier line.
7. **Note and count parity**, pinned by a dry-run-vs-apply test.
8. **Compiled live cases** for module + children and for a cluster with an `also` child.
9. **Docs at wrap:** the feature doc rows and limitations, `docs/path-survey.md`, `docs/facades.md`, plan-schema prose. Delete or narrow the claimed todos.

## Acceptance Criteria

- [ ] `move_module_to_crate` of a module with `mod b;` (`a/b.rs`), `mod c;` (`a/c/mod.rs` → `a/c/e.rs`) and an inline `mod i { mod d; }` moves all five files to the same relative places in the destination. Nothing stays in the origin, the destination root declares only `a`, and the workspace compiles with its tests ([Rust code restructuring](../rust-code-restructuring.md)).
- [ ] A carried child's `crate::` paths are re-pointed and its `super::` paths inside the tree are byte-identical. A crate only a child names joins the destination's `[dependencies]`, or `[dev-dependencies]` when only its `#[cfg(test)]` code names it.
- [ ] `move_cluster_to_crate` with a child in `also` nests the child under its parent. The moved parent's `mod child;` line is unchanged, no file in the destination names the destination's crate, and the workspace compiles.
- [ ] With `reexport: "none"`, a caller outside the moved tree of an item in a carried child is re-pointed to `<dest>::a::b::Item`.
- [ ] A child body reaching a module that stays behind is refused by `check` and `apply` with the same message. So is a missing child file, a `#[path]` child, an existing target path, and a child moved separately by another operation of the plan; plain `check` reports each. A restricted child reached from outside the tree is refused by `check --deep` and `apply`. Each names the file and writes nothing.
- [ ] `pub(crate) mod x;`, `pub(super) mod x;` and `pub(in crate::p) mod x;` are moved, not refused. The glob facade left in their place has the same visibility, and `check` no longer reports "declares no `mod x`".
- [ ] A destination root declaring `pub(crate) mod x;` is reported as a merge.
- [ ] Two glob moves of `pub(crate)` modules to one destination leave one `pub(crate) use dest::{a, b};` line.
- [ ] `..crate::old::f()` in a struct update and `0..crate::old::MAX` in a range are re-pointed. A `..` path into a module staying behind is refused by the body precondition.
- [ ] For a module with children, `apply --dry-run`'s `-> N file(s)` equals `apply`'s, and the run prints the "`N` file(s) move with `<module>`…" note.
- [ ] Existing crate-move suites pass unchanged: a lone nested member still lands at the root, and `pub mod` facades are unchanged.
- [ ] Tests pass for `tddy-code-restructuring` (scoped `./test -p tddy-code-restructuring`; CI for the rest).

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-move-children.md` (follows PRD review)
- Discovery: [2026-10-09-reshape-move-children-initial-discovery.md](../../../dev/1-WIP/2026-10-09-reshape-move-children-initial-discovery.md)
- Todos this closes:
  - [move-cluster-to-crate-leaves-a-modules-directory-children-behind](../../../dev/todo/2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md): narrowed at wrap to its `libc` sub-item, which is node 9's.
  - [module-move-strands-its-directory-child-and-the-cluster-leaves-a-dangling-self-reexport](../../../dev/todo/2026-10-08-restructure-module-move-strands-its-directory-child-and-the-cluster-leaves-a-dangling-self-reexport.md)
  - [move-cluster-ignores-also-members-that-are-directory-children-and-their-crates](../../../dev/todo/2026-10-08-restructure-move-cluster-ignores-also-members-that-are-directory-children-and-their-crates.md): narrowed at wrap to create-crate, `async-trait` and `libc`, which are node 9's.
  - [move-to-crate-does-not-read-a-restricted-mod-declaration](../../../dev/todo/2026-10-08-restructure-move-to-crate-does-not-read-a-restricted-mod-declaration.md)
  - [hand-widened-mod-declarations-before-engine-moves](../../../dev/todo/2026-10-08-hand-widened-mod-declarations-before-engine-moves.md)
  - [cluster-move-misses-body-paths-and-writes-self-referencing-test-reexports](../../../dev/todo/2026-10-08-restructure-cluster-move-misses-body-paths-and-writes-self-referencing-test-reexports.md)
- Closed as already done (developer-approved deletion at wrap):
  - [macro-expansion-as-a-restructure-operation](../../../dev/todo/2026-09-09-macro-expansion-as-a-restructure-operation.md)
  - [restructure-defects-from-the-connection-service-split](../../../dev/todo/2026-09-09-restructure-defects-from-the-connection-service-split.md)
- Package docs: `packages/tddy-code-restructuring/docs/path-survey.md`, `packages/tddy-code-restructuring/docs/facades.md`, `packages/tddy-code-restructuring/docs/same-crate-moves.md` (the `reparent_module` prior art).
- Successor: `feature/reshape/tests-follow` (real edge 5→14).
