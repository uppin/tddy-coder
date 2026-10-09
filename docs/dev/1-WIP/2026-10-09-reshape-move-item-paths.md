# Changeset: same-crate moves and `retarget_impl` read a `use` the way rustc resolves it

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Bug Fix (engine correctness: path rebasing, the destination's imports, `retarget_impl` S6)
**Stack**: `#reshape` 11/19, branch `feature/reshape/move-item-paths`, wave 1. PR title:
`fix(code-restructuring): moves and retarget_impl resolve imports as rustc does (#reshape 11/19)`.
Base in the linear stack: `feature/reshape/apply-robust` (K=10). **Real edges**: none — no node's behaviour is
consumed, and no node consumes this one's. The base is a line position only (nodes 1–12 share
`item_move/*` and `crate_move/source_scan*` textually, not behaviourally).

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-09-reshape-move-item-paths-initial-discovery.md) (Exploration 1: the
whole-work backlog discovery; Exploration 2: this node's `use`-reading findings, E2.1–E2.7).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rn 'Claimed by:' packages/tddy-code-restructuring/docs/code-issues/` finds one record, whose value is
`none` (`broken-restructure-anchors-empty-outline.md`, node `anchors-outline`'s): **no 🚧 claimed issue is in
the path, no wait-or-proceed fork.**

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-10-07-restructure-retarget-impl-refuses-a-relative-import-of-its-target-type.md](../todo/2026-10-07-restructure-retarget-impl-refuses-a-relative-import-of-its-target-type.md) | ✅ **RESOLVED HERE** | S6 resolves each `use` leaf against the file's module before comparing (rule S6′). Deleted at wrap, with the stale `TODO(restructure-retarget-impl-s6)` comment in `packages/tddy-agent-launch/src/svc_resume_claude_cli_session.rs:15-16` (F4: comment only; the `crate::` spelling stays) |
| [2026-10-08-restructure-move-item-miswrites-a-facade-path-in-a-moved-impl-block.md](../todo/2026-10-08-restructure-move-item-miswrites-a-facade-path-in-a-moved-impl-block.md) | partial (claimed by `tidy-facades`) | **Facade-path half**: rules R2, R3, R7. Wrap removes that half (and its "Spell a path through a facade …" clause) from the entry; the tidy-skipped half stays for `feature/reshape/tidy-facades`, which deletes the file if it wraps after this node |
| [2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle.md](../todo/2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle.md) | partial (claimed by `widen-same-crate`) | **Alias and glob bullets**: R5, R6, R8 and the destination alias rule D1. Wrap deletes those two bullets; emptied directories and `pub use`-chain widening stay for `widen-same-crate`; import placement and "one new module per `name`" stay open |
| [2026-10-04-restructure-move-item-copies-the-whole-use-header.md](../todo/2026-10-04-restructure-move-item-copies-the-whole-use-header.md) | — Unrelated | Which `use` items travel is not changed |
| [2026-10-04-restructure-reparent-module-first-cut-limits.md](../todo/2026-10-04-restructure-reparent-module-first-cut-limits.md) | — Unrelated (`widen-same-crate`'s) | `reparent_module` gains R1–R8 through the shared rebasing; none of the entry's items is addressed |
| [2026-10-06-restructure-item-move-assemble-past-500.md](../todo/2026-10-06-restructure-item-move-assemble-past-500.md) | ⚠ **DURING** (`oversized-files`'s) | `item_move/assemble.rs` (507) changes by **zero** lines net: the `imports` closure gains one argument on its existing line and `rebase::edits(…)` gains a `?` |
| Function-size list (whole-work discovery, Exploration 3): `rebase.rs` `path_edit` (60), `sites.rs` `rewrite_statement` (66), `assemble.rs` `moved_text` (60), `module_items.rs` `items_of_module` (64) | ⚠ **DURING** (`fn-sizes-rest`'s) | None grows. `path_edit`'s follow block (`rebase.rs:148-157`) becomes one call to the new `respelled`, so it shrinks; `items_of_module`'s `use` arm becomes a call to the new `read_use`, so it shrinks; `rewrite_statement`'s drop branch calls the new `destination_import` on its existing line |
| `packages/tddy-code-restructuring/docs/code-issues/` (`oversized-file-backends-rust.md`, `oversized-file-test-binary.md`, `complexity-rust-facade-lines.md`, `dead-code-plan-filehint-modified.md`, `broken-restructure-anchors-empty-outline.md`) | — Unrelated | `backends/rust.rs` and `crate_move/test_binary.rs` are not edited |

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md);
  new `src/backends/rust/item_move/use_path.rs`; `src/backends/rust/item_move.rs` (one `mod`);
  `src/backends/rust/item_move/{preflight,bindings,rebase,sites,assemble}.rs`;
  `src/backends/rust/module_reparent/assemble.rs` (the `imports` closure, `?`);
  `src/backends/rust/retarget_impl/imports.rs`; `src/crate_move/source_scan.rs` (`UseLeaf.visibility`) and
  `src/crate_move/source_scan/module_items.rs` (`read_use`, the `use` item's visibility).
  Tests: new `tests/move_item_paths_acceptance.rs`; `tests/retarget_impl_acceptance.rs` (three tests and one
  fixture helper). Registration: `.config/rust-e2e.filterset`, `.config/nextest.toml` (`rust-analyzer`
  group) — the new binary only.
  Docs at wrap: [same-crate-moves.md](../../../packages/tddy-code-restructuring/docs/same-crate-moves.md)
  (preflight row, Limits), [retarget-impl.md](../../../packages/tddy-code-restructuring/docs/retarget-impl.md)
  (`imports.rs` row, S6), [rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md)
  (Same-crate moves › **Imports** and **What a caller keeps…**, `### retarget_impl`, `## Known limitations`).
- **`tddy-agent-launch`**: `src/svc_resume_claude_cli_session.rs` — the two-line stale `TODO` comment is
  removed at wrap (F4). No code change.
- **`tddy-tools`, `tddy-index-daemon`**: no change.

## Related Feature Documentation

- [PRD-2026-10-09-reshape-move-item-paths.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-move-item-paths.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md) — `## Rust operations (v1)` › Same-crate moves, `### retarget_impl`; `## Known limitations`

## Summary

One reading of a `use` path replaces the engine's text reading: a head of `crate`/`self`/`super` is
resolved against the importing module, any other head is local when that module binds it and an extern crate
otherwise (Rust 2018). On it: a `self::`/`super::` path in code that `move_item` or `reparent_module` moves is
kept as written when it still resolves from the destination, followed one hop through the import that binds
the name otherwise (plain, aliased, glob, extern crate), and refused when a glob cannot confirm it; a
destination's aliased import of the moving item keeps its alias; `retarget_impl` stops refusing a relative
spelling of the type it imports.

## Background

`#carve` 21/21 (PR #536) failed its compile gate on `super::tddy_session_split::service_util::…` (`E0433`),
written by `move_item` from `super::service_util::…` because `pub use tddy_session_split::{…service_util…};`
was read as a child module of `connection_service`. `#carve` 20/21 was refused because
`use super::launch_ports::LaunchSessions;` is not text-equal to `use crate::…::LaunchSessions;`. The
2026-10-05 limits list a `super::Name` through an aliased or a glob import as not followed (both write a
private import's path, `E0603`), and describe an aliased destination import as a clash refusal — in the code
it is dropped and the gate fails on the unbound alias (E2.4). All are hand fixes on the plans the `#reshape`
and crate-split stacks will run most.

## Responsibility

- `item_move::use_path`: the one Rust-2018 reading of a `use` path (in-crate vs extern), replacing
  `preflight::resolved_from` at its three callers.
- `UseLeaf.visibility`, read by `items_of_module` for top-level `use` items (F1).
- `bindings::import_target`: the one-hop answer for how a module binds a name it does not define — visible at
  the destination, an in-crate target (plain, aliased, confirmed glob), an extern path, or unconfirmed (F3).
- `rebase::path_edit`: rules R1–R8; `rebase::edits` returns `Result` so R8 refuses (F2).
- `sites::rewrite_statement`: D1, the destination keeps an aliased import of the moving item.
- `retarget_impl::imports::the_use`: rule S6′.
- One new live binary, registered in `.config/rust-e2e.filterset` and the `rust-analyzer` group.

## The rules (the contract)

**U — reading a `use` path** (`use_path::resolved(segments, at, local)`), `at` the importing module below the
crate root, `local` that module's `ModuleItems`:

| Head | Result |
|---|---|
| `crate` | `InCrate(rest)` |
| `self`, `super`… | `InCrate` against `at`; `None` when a `super` climbs above the root |
| a name `local` binds (a child `mod`, a defined item, a non-glob `use` binding) | `InCrate(at ++ segments)` |
| any other name | `Extern(segments)` (an extern crate, as rustc 2018 reads it) |

A leading `::` is already dropped by the scanner (`source_scan.rs:123-128`) and so reads as a name; when the
module binds no such name it is `Extern`, which is what `::` means.

**R — a relative path through an import.** For each `self::`/`super::` path in moved code that reaches module
`M` and names `Name` (`rebase::path_edit`), in order; `to` is the destination module:

| # | When | Written |
|---|---|---|
| R1 | `M` defines `Name` or declares it as a child `mod` (`import_target` → `None`) | `relative_to(M, to)` + `Name` (today) |
| R2 | `to` is `M` or below it (`to.starts_with(M)`) | `relative_to(M, to)` + `Name` — byte-identical when the `super::` count is unchanged; no lookup |
| R3 | `M`'s `use` binding `Name` has a visibility whose `Scope` (`scope::Scope::parse(vis, M)`) contains `to` → `Imported::Visible` | `relative_to(M, to)` + `Name` |
| R4 | private plain `use p::Name;`, `p` in this crate → `Imported::InCrate { module: p, name: "Name" }` | `relative_to(p, to)` + `Name` (today) |
| R5 | private `use p::Real as Name;` → `InCrate { module: p, name: "Real" }` | `relative_to(p, to)` + `Real` — the edit spans prefix **and** name |
| R6 | private `use p::*;` in this crate, exactly one glob of `M` whose module `p` binds `Name` (defines it, declares it, or a non-glob `use` binds it; read with `find_module` + `items_of_module`) → `InCrate { module: p, name: "Name" }` | `relative_to(p, to)` + `Name` |
| R7 | private `use` whose head is `Extern` → `Imported::Extern { path, shadowed }` | `kernel::util::` + `Name`; `::kernel::util::` + `Name` when `to`'s own module binds the crate's name (`shadowed`) |
| R8 | `M` binds `Name` only by globs, none or more than one of which confirm it → `Imported::Unconfirmed(why)` | **refused** (`seam_refusal`): ``<file>:<line>: `<written path>` reaches `Name` through a glob import of `<M>` that cannot be followed (<why>); write the path to the module that defines it and plan again`` — nothing written, by `check --deep` and by `apply` |

A module file that cannot be read stays as today (R1's spelling): the compile gate decides. One hop only
(F3): a target that is itself a re-export is written as the import names it, not chased to its definition.

**D1 — the destination's own import of the moving item.** `sites::rewrite_statement` with `drop`
(`sites.rs:354-358`, group members `:381-387`): an un-renamed import (`use a::name;`, `{name}`) is removed
(today); an aliased one (`use a::name as other;`, `{name as other}`) is written `use self::name as other;`
with the statement's visibility kept (`pub(crate) use self::name as other;`).

**S6′ — `retarget_impl`'s binding check** (`retarget_impl/imports.rs`): with `here` the file's module below
the crate root (`module_path_of(..)[1..]`) and `target = to_type module ++ [Name]`, a top-level `use` leaf
binding `Name` (non-glob) means:

- `resolved(leaf.segments, here, items) == Some(InCrate(target))` → already bound: no `use` written, no refusal;
- anything else (`InCrate` of another path, `Extern`, `None`) → today's `E0255` refusal, today's message.

A `use` of `Name` under an alias (`use crate::m::T as U;`) binds `U`, not `T`: unchanged (a `use` is written).

## Boundaries

- **No multi-hop following** (F3) and no change to `crate_move::reexports` (its own head rule, E2.1, is a
  proposed todo), `canonical_paths`, `repoint_facade_imports` or any cross-crate move.
- **No change to the clash check's verdicts** (`preflight::taken_by_something_else`, `reexports`): only the
  reading under them is replaced; an extern import still counts as a clash, now for the right reason.
- **No `reparent_module` clash-check change** (`module_reparent/survey.rs` `name_taken`, F5 → todo).
- **No widening** of fields, `impl` members or a re-parented tree (`widen-same-crate`), no tidy change
  (`tidy-facades`), no static-`check` S6 (it still needs the server).
- **No new plan field, operation, flag or wire message.** No dependency added.
- **Registers only `move_item_paths_acceptance`**; the older unregistered same-crate suites are
  `feature/reshape/widen-same-crate`'s (developer decision, 2026-10-09).
- **No function on the function-size list grows** (`path_edit`, `rewrite_statement`, `moved_text`,
  `items_of_module`).

## Dependencies

This node consumes no other node's behaviour: it is greenable on `master` alone; its base,
`feature/reshape/apply-robust`, is a line position for `gh stack`.

## Draft PR contract

Published with the wave-2 contract commit (the first push of this PR carrying code); **owned surface**:

- New `backends::rust::item_move::use_path`:
  `pub(in crate::backends::rust) enum UsePath { InCrate(Vec<String>), Extern(Vec<String>) }` and
  `pub(in crate::backends::rust) fn resolved(segments: &[String], at: &[String], local: &ModuleItems) -> Option<UsePath>`.
  `preflight::resolved_from` is removed; its callers (`preflight.rs:188,223`, `bindings.rs:36`) call
  `resolved`.
- `crate_move::source_scan::UseLeaf` gains `pub(crate) visibility: Option<String>` — the `pub…` text of the
  `use` item holding the leaf, `None` for a private one; set by `items_of_module` (through the new private
  `module_items::read_use(scan: &Scan<'_>, at: usize, items: &mut ModuleItems) -> usize`), `None` from every
  other reader of `use_tree`.
- `item_move::bindings`:
  `pub(in crate::backends::rust) enum Imported { Visible, InCrate { module: Vec<String>, name: String }, Extern { path: Vec<String>, shadowed: bool }, Unconfirmed(String) }`;
  `pub(in crate::backends::rust) fn import_target(workspace: &Workspace<'_>, package: &Package, module: &[String], name: &str, destination: &[String]) -> Option<Imported>`.
- `item_move::rebase`: `type Imports<'a> = dyn Fn(&[String], &str, &[String]) -> Option<Imported> + 'a`;
  `Modules` gains `pub(in crate::backends::rust) file: &'a str`;
  `pub(in crate::backends::rust) fn edits(…same parameters…) -> Result<Vec<Edit>>`; new private
  `fn respelled(at: usize, cursor: usize, named: &str, arrives_at: &[String], to: &[String], modules: &Modules<'_>) -> Result<Option<Edit>>`.
- `item_move::sites`: new private `fn destination_import(head: &str, name: &str, alias: &str) -> String`.
- `retarget_impl::imports::the_use` keeps its signature.
- Failing tests: the 21 red ones under "Acceptance tests" (22 is a green pin).

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes, on `master`.
**Concurrent with:** every other wave-1 node — `feature/reshape/widen-same-crate`, `multi-seam-extract`,
`tidy-facades`, `extract-method-clean`, `move-children`, `methods-leave-type`, `move-widen`,
`move-grouped-use`, `new-crate`, `apply-robust`, `anchors-outline` (all `feature/reshape/…`). Textual overlap
to expect at rebase: `item_move/{assemble,sites,preflight}.rs` with `widen-same-crate`;
`crate_move/source_scan*` with `move-children`, `move-grouped-use`, `oversized-files`.
**Blocks:** none.
Real dependency edges (whole stack): `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`,
`17→18`, `6→18`, `4→19`, `17→19`. None touches K=11.

## Successor PRs

None — no node consumes this one's behaviour.

## Scope

- [ ] **`use` reading**: `use_path.rs` (U), `UseLeaf.visibility`, `read_use`; callers of `resolved_from` moved over
- [ ] **One-hop lookup**: `Imported`, `import_target` (R3–R8 answers), both closures
- [ ] **Rebasing**: `path_edit` R1–R8 via `respelled`, `edits` → `Result`, `Modules.file`
- [ ] **Destination alias**: D1 in `rewrite_statement` via `destination_import`
- [ ] **`retarget_impl`**: S6′
- [ ] **Registration**: `move_item_paths_acceptance` in `.config/rust-e2e.filterset` and the `rust-analyzer` group
- [ ] **Package documentation** at wrap (list under Affected Packages); todos deleted/narrowed; stale marker removed
- [ ] **Testing**: acceptance tests pass; `./test -p tddy-code-restructuring`, scoped; CI for the rest
- [ ] **Code quality**: `cargo check -p tddy-code-restructuring --all-targets`, `cargo clippy -p tddy-code-restructuring -- -D warnings`, `cargo fmt`; no listed function grows; every touched file ≤ 500 production lines

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- `preflight::resolved_from` (`item_move/preflight.rs:235-250`) reads every head other than `crate` as
  relative to the importing module — an extern crate included.
- `bindings::import_target` (`item_move/bindings.rs:16-37`) follows only an un-renamed, non-glob `use`
  (`:32-35`), with no regard to the import's visibility or to where the destination is.
- `rebase::path_edit` (`item_move/rebase.rs:99-158`) follows that answer unconditionally and keeps it only
  when its last segment is the written name (`:149-156`); it never rewrites the name token; `edits` cannot
  refuse.
- `sites::rewrite_statement` drops a destination's import of the moving item with its alias
  (`item_move/sites.rs:354-358`, `:381-387`).
- `retarget_impl::imports::the_use` compares `use` segments as text with `["crate", …]`
  (`retarget_impl/imports.rs:41-45`) and refuses any other binding of the name (`:50-57`).
- `UseLeaf` (`crate_move/source_scan.rs:245-250`) carries no visibility.

### State B (Target)

The `#carve` 21/21 and 20/21 plans apply without a hand edit; a `super::` path through an aliased, glob or
extern import of a module the code leaves compiles at its destination or is refused before anything is
written; a destination keeps a renamed import of what moves into it.

### Delta (What's Changing)

#### `tddy-code-restructuring`
- **New** `item_move/use_path.rs` (~70 production lines + tests): `UsePath`, `resolved`.
- `item_move.rs`: `mod use_path;`.
- `crate_move/source_scan.rs`: `UseLeaf.visibility` (two struct literals in `use_tree` set `None`).
- `crate_move/source_scan/module_items.rs`: the `use` arm of `items_of_module` → `read_use` (reads the
  `pub`/`pub(…)` tokens before `use`, stamps the new leaves).
- `item_move/preflight.rs`: `resolved_from` removed; `taken_by_something_else` and `reexports` call
  `use_path::resolved` with the module's `ModuleItems` (they already hold it).
- `item_move/bindings.rs`: `Imported`; `import_target` gains `destination` and returns R3–R8's answer
  (~60 more lines; the glob confirmation is its own function).
- `item_move/rebase.rs`: `Imports`, `Modules.file`, `edits -> Result`, `respelled` (R2–R8; R5's edit covers
  the name); the two unit tests' closures follow the new signature.
- `item_move/sites.rs`: `destination_import`, called from both drop sites.
- `item_move/assemble.rs` (`moved_text`) and `module_reparent/assemble.rs`: closure argument, `file:` field,
  `?` — no line growth.
- `retarget_impl/imports.rs`: S6′ (`items` read once; `here` from `module_path_of`).

#### Configuration
- `.config/rust-e2e.filterset` and `.config/nextest.toml`: `binary(move_item_paths_acceptance)`.

## Implementation milestones

- [ ] **M1** `use_path` + `UseLeaf.visibility`; tests 11–14
- [ ] **M2** S6′; tests 18–20, 8–9, 22
- [ ] **M3** `Imported` / `import_target`; tests 15–17
- [ ] **M4** `path_edit` R1–R8, `edits → Result`; tests 10, 1–6
- [ ] **M5** D1; tests 21, 7
- [ ] **M6** registration; scoped gate; length and function-size check
- [ ] **M7** docs staged; todos narrowed/deleted; stale marker removed

## Testing plan

### Testing Strategy

**Primary: library level, no server.** The `use` reading, the one-hop lookup, the rule table, D1 and S6′ are
all lexical: unit tests in the files that own them (the `rebase.rs` tests already drive `edits` with an
`imports` closure; `bindings.rs`, `use_path.rs` and `retarget_impl/imports.rs` tests run over a temp-dir
workspace with an `Overlay`, as `crate_move/reexports.rs:263-317` does). **Live, thin:** one new binary drives
`move_item` / `reparent_module` through the runner with `assert_compiles_with_its_tests` as the oracle, and
three `retarget_impl` cases join its registered suite.

#### Option 1 (chosen): unit tests in the owning modules
**Trade-off**: exact and fast; does not prove the tree compiles. **Location**: the `src/…` files named below.

#### Option 2 (chosen, thin): live fixture crates
Fixtures from `tests/same_crate/mod.rs` (`an_app_holding`, `an_app_over_a_kernel`). **Location**: new
`packages/tddy-code-restructuring/tests/move_item_paths_acceptance.rs` (registered) and
`packages/tddy-code-restructuring/tests/retarget_impl_acceptance.rs` (already registered).

#### Option 3 (rejected): the older same-crate suites
They are not registered in the e2e set or the test group yet (that is `widen-same-crate`'s), so a test added
there would run in the wrong CI leg until then.

### Coverage Requirements

- [ ] U: each head kind; `super` above the root
- [ ] R1–R8 each, at unit level; R2, R5, R6, R7, R8 and `reparent_module` live
- [ ] D1 plain and grouped; S6′ `super::`, `self::`, child-relative, and a different item still refused
- [ ] Actual effects: bytes on disk; `cargo check --all-targets`; an R8 refusal leaves the tree byte-identical

## Acceptance tests

Names read as behaviour specifications. **1–21 are red on `master`**, each for the reason given; **22** is a
green pin.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/move_item_paths_acceptance.rs` (new; live rust-analyzer; registered)

1. `a_move_into_a_sibling_keeps_a_path_through_the_parents_facade_of_another_crate` — `an_app_over_a_kernel`;
   `app::host` holds `pub use kernel::util;`, `host/observer.rs` has `super::util::answer()`; `move_item` with
   `name` creates `app::host::wiring`. The moved line is byte-identical; no `super::kernel::`; compiles with its
   tests. *Red*: written `super::kernel::util::` (`E0433`), the gate fails.
2. `a_move_out_of_a_module_that_privately_imports_another_crate_writes_the_crates_path` — `host.rs` holds
   private `use kernel::util;`; `host/observer.rs` → `app::split`; the moved line reads `kernel::util::answer()`;
   compiles. *Red*: `super::host::kernel::util::` (`E0433`).
3. `a_reparented_module_names_another_crate_its_old_parent_imported_by_the_crates_path` — same shape through
   `reparent_module` of `host::observer` under `app::split`. *Red*: as 2.
4. `a_path_through_the_old_modules_aliased_import_names_the_item_it_brings_in` — `host.rs` holds
   `use crate::types::Config as Settings;`, the moved code writes `super::Settings`; arrives as
   `super::types::Config`; compiles. *Red*: `super::host::Settings` (`E0603`).
5. `a_path_through_the_old_modules_glob_import_names_the_module_that_holds_the_item` — `host.rs` holds
   `use crate::types::*;`, the moved code writes `super::Config`; arrives as `super::types::Config`;
   compiles. *Red*: `super::host::Config` (`E0603`).
6. `a_path_through_a_glob_that_cannot_confirm_the_name_is_refused_by_check_deep_and_by_apply_and_nothing_is_written`
   — `host.rs` globs `crate::a::*`, and `a` only globs `crate::b::*` where `Config` is defined; `check --deep`
   and `apply` both refuse naming `src/host/worker.rs:<line>` and `super::Config`; every file byte-identical.
   *Red*: no refusal; `apply` writes `super::host::Config` and stops at the gate with the edit on disk.
7. `a_destination_that_imports_the_moving_item_under_an_alias_keeps_the_alias_bound` — `answers.rs` holds
   `use crate::pairing::peer as check;` and calls `check(code)`; `peer` moves into `app::answers`; the file
   holds `use self::peer as check;`, `peer` is defined once, compiles. *Red*: the import is dropped (`E0425`).

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/retarget_impl_acceptance.rs` (existing, registered; new helper `a_crate_whose_svc_host_reads` with the type in `app::svc::host`)

8. `retargets_a_block_in_a_file_that_imports_the_new_type_by_a_super_path` — `src/svc/host.rs` holds
   `use super::roster::Roster;`; `<Host>` → `app::svc::roster::Roster` applies; exactly one `use` binds
   `Roster`; compiles. *Red*: S6 refuses (`E0255`, "already bound … to something else").
9. `retargets_a_block_in_a_file_that_imports_the_new_type_from_a_child_module_by_a_relative_path` —
   `src/svc.rs` holds `pub mod roster;` and `use roster::Roster;` (2018 child-relative) beside `impl Host`;
   applies; compiles. *Red*: S6 refuses.

### `tddy-code-restructuring` — library level, no server (unit tests in the owning modules)

10. `packages/tddy-code-restructuring/src/backends/rust/item_move/rebase.rs` — one test per row, with an
    `imports` closure: `keeps_a_super_path_when_the_destination_is_inside_the_module_it_reaches` (R2, the
    `#carve` 21/21 shape), `keeps_a_super_path_through_an_import_visible_at_the_destination` (R3),
    `follows_an_aliased_import_and_writes_the_name_it_brings_in` (R5),
    `follows_a_confirmed_glob_import_to_the_module_that_binds_the_name` (R6),
    `writes_an_extern_import_as_the_crates_own_path` and `roots_an_extern_path_when_the_destination_shadows_the_crate_name` (R7),
    `refuses_a_path_through_an_unconfirmed_glob_naming_the_file_and_the_line` (R8);
    `follows_an_import_of_the_module_a_super_path_names` (R4) stays, on the new closure. *Red*: the closure
    type, `Modules.file` and `edits`' `Result` do not exist; R2/R3/R5–R8 behaviour is absent.
11. `packages/tddy-code-restructuring/src/backends/rust/item_move/use_path.rs` —
    `a_head_the_module_does_not_bind_is_an_extern_crate`. *Red*: module does not exist.
12. same file — `a_head_naming_a_child_module_an_item_or_an_import_of_the_module_is_local`
    (`use inner::X`, `use Mode::Fast` with `enum Mode`, `use alias::Y` with `use crate::x as alias`). *Red*: as 11.
13. same file — `crate_self_and_super_heads_resolve_against_the_module_and_climbing_above_the_root_is_none`
    (replaces `preflight.rs`'s `reads_a_super_import_against_the_module_it_is_written_in`). *Red*: as 11.
14. `packages/tddy-code-restructuring/src/crate_move/source_scan/module_items.rs` —
    `a_top_level_use_records_its_visibility_on_every_leaf` (`pub use`, `pub(crate) use a::{b, c}`,
    `pub(in crate::x) use`, private `use` → `None`). *Red*: no `visibility` field.
15. `packages/tddy-code-restructuring/src/backends/rust/item_move/bindings.rs` —
    `a_pub_crate_import_reachable_from_the_destination_is_visible` (R3) and
    `a_pub_super_import_the_destination_is_outside_of_is_followed` (R3 negative → R4). *Red*: no `Imported`,
    no `destination` parameter.
16. same file — `an_aliased_import_answers_the_name_it_brings_in` (R5) and
    `an_extern_import_answers_the_crates_path_and_whether_the_destination_shadows_it` (R7). *Red*: alias
    skipped; extern read as relative.
17. same file — `a_glob_counts_only_when_exactly_one_glob_module_binds_the_name` (one → `InCrate`; none and two →
    `Unconfirmed`). *Red*: globs never considered.
18. `packages/tddy-code-restructuring/src/backends/rust/retarget_impl/imports.rs` —
    `a_super_import_of_the_new_type_needs_no_second_use`. *Red*: refused (`E0255`).
19. same file — `a_self_or_child_relative_import_of_the_new_type_needs_no_second_use`. *Red*: refused.
20. same file — `an_import_of_another_item_or_another_crate_under_the_name_is_still_refused`
    (`use super::other::Roster;`, `use kernel::Roster;`). *Red*: the test module and its workspace fixture do
    not exist yet (both verdicts already hold; this pins them at unit level).
21. `packages/tddy-code-restructuring/src/backends/rust/item_move/sites.rs` —
    `the_destination_keeps_an_aliased_import_of_the_moved_item_as_a_self_import` (plain and group member;
    beside `keeps_an_alias_and_a_visibility_of_a_plain_use`). *Red*: the statement is emptied.

### Green pin

22. `packages/tddy-code-restructuring/tests/retarget_impl_acceptance.rs` —
    `still_refuses_a_super_import_of_a_different_type_of_the_same_name` (`src/svc/host.rs` holds
    `use super::other::Roster;`); **green today** — pins that S6′ does not turn a real clash into a silent
    second binding. The existing `refuses_a_use_that_would_clash_with_a_type_the_file_already_binds` stays
    unchanged.

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by the developer (2026-10-09, wave-1 PRD review, quoted from the stack brief): "All 12 wave-1 PRDs
approved. Every node's own F-decisions: take the agent's recommendation unless overridden below." None is
overridden for this node. "The unregistered live rust-analyzer suites … → node 1 registers all of them …
Other nodes register only the suites they add." "Nodes must not grow functions on nodes 16/19's list."

**Taken** (the PRD's F-decisions, recommendation adopted):
- **F1 — read a `use`'s visibility in the scanner** so R3 keeps a facade as written. Taken: `UseLeaf.visibility`.
- **F2 — a glob that cannot confirm the name refuses** (R8), rather than writing a path with a note.
- **F3 — one hop**, not `crate_move::reexports::followed`'s full chain (which can land in a private module).
- **F4 — remove the stale `TODO(restructure-retarget-impl-s6)` comment** in `tddy-agent-launch` at wrap.
- **F5 — `reparent_module`'s clash check is deferred** to a todo (written by this node).

**OPEN** (each with a recommendation):
- **F6 — D1's spelling.** (a) **`use self::name as other;`** — *recommended*: says "this module's own item",
  survives a later move of the whole module; (b) `use crate::<dest>::name as other;` — what non-destination
  callers get, but repeats the module's own path inside it.
- **F7 — R7 when the destination shadows the crate's name.** (a) **`::kernel::…`** — *recommended*, the
  2018 spelling rustc accepts everywhere; (b) refuse.

Decisions taken by this plan: `resolved_from` is replaced, not wrapped; `rebase::edits` returns `Result`
(one refusal path for R8); `UseLeaf.visibility` is set only by `items_of_module`.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
(empty)

### From @red (TDD Red Phase)
(empty)

### From @validate-changes (Change Validation)
(empty)

### From @validate-tests (Test Quality)
(empty)

### From @prod-ready (Production Readiness)
(empty)

### From @analyze-clean-code (Code Quality)
(empty)

### From @refactor (Completed Refactorings)
(empty)

## Validation Results

(empty; populated during development)

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-move-item-paths-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the scoped gate (`./test -p tddy-code-restructuring`) — verify 100% pass; CI for the rest
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p tddy-code-restructuring -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-09-reshape-move-item-paths-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
