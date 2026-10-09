# Changeset: cross-crate moves take the sibling test modules that test only the moved code

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Feature (engine capability for `move_module_to_crate` / `move_cluster_to_crate`) plus one defect fix
**Stack**: `#reshape` 14/19, branch `feature/reshape/tests-follow`, green wave 2. PR title:
`feat(code-restructuring): crate moves take the test modules that test only moved code (#reshape 14/19)`.
**PR**: [#611](https://github.com/uppin/tddy-coder/pull/611).
**Base branch** in the linear stack: `feature/reshape/move-impl-members` (K=13). That is a line position only, not a dependency.
**Real edges:** one in, none out. `move-children → tests-follow` (5→14): this node extends node 5's carried-file set and travelling set with sibling `#[cfg(test)]` modules, and lands them with node 5's relocation rule.

## Initial Discovery

The codebase exploration that grounded this plan is in [initial-discovery.md](./2026-10-09-reshape-tests-follow-initial-discovery.md):

- Exploration 1 is the whole-work discovery.
- Exploration 2 is this node's, with every claim verified at file:line against `4a5c42b1b`.

State A below is distilled from that file. Grep traces are not repeated here.

## Prerequisites

`grep -rl 'Claimed by:'` over `packages/tddy-code-restructuring/docs/code-issues/` finds no issue claimed by an open PR in this path.

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-10-08-restructure-cluster-move-strands-test-modules-of-the-moved-code.md](../todo/2026-10-08-restructure-cluster-move-strands-test-modules-of-the-moved-code.md) | ✅ **RESOLVED HERE** | Its three sibling files (`conversation_spawn_wiring_tests`, `host_session_socket_tests`, `session_acting_identity_tests`) follow; its fourth (`claude_cli_spawn_steps_tests`, declared inside the moved file) is carried by node 5 and read as test code here; `stack_child_spawn_tests` stays with a note. Its "or list it in `check --deep` as stranded" is the staying note. The `[dev-dependencies] tempfile` hand fix is the test-throughout rule. Deleted at wrap |
| [2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md](../todo/2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md) item M | — Unrelated (node 2 claims the file; M stays open) | M is a same-crate `extract_module` dropping parent imports that sibling test modules reach through `use super::*`. This node does not touch `extract_module` and does not change what a staying test module's `use super::*` reaches |
| [2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md](../todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md) | — Unrelated (node 15's, developer reassignment) | The budget counter is not touched |
| [2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md](../todo/2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md) | — Unrelated (node 8 claims it) | The stranded-sibling finding is not extended to test modules: a test module that cannot follow stays and compiles through the facade or the re-pointed callers, so it is a note, not a finding |
| Function-size list (whole-work discovery, Exploration 3): `resolve_cluster` (`cluster.rs:111`), `sightings` (`source_scan/sighting_walk.rs:33`), `items_of_module` (`source_scan/module_items.rs:32`), `resolve_opening` (`backends/rust.rs:1245`) | ⚠ **DURING** | None grows. The declaration reader is a new function in a new file; `resolve_cluster` gains a call only where a line of its own leaves (the travelling set moves into `test_modules::travelling`); `sightings` is not edited (test-throughout is applied to the survey's result) |

## Affected Packages

- **`tddy-code-restructuring`**:
  - **New:** `src/crate_move/test_modules.rs` (considered declarations, classification, follow edits, notes, static refusal); `src/crate_move/source_scan/test_declarations.rs` (the declaration reader).
  - `src/crate_move.rs`: `mod test_modules;`; `cluster_resolution` appends the test-module notes.
  - `src/crate_move/source_scan.rs`: `mod test_declarations;` and its re-export.
  - `src/crate_move/header.rs`: `FileInTree`, `Gate`, `repointed_header_in`; `reach` reads `defined_at` and the file's own tree root.
  - `src/crate_move/cluster.rs`: the travelling set and the edit include following test files; `member_changes` picks the gate per carried file.
  - `src/crate_move/preconditions.rs`: `unrunnable` calls `test_modules::unfollowable`.
  - Docs at wrap: [path-survey.md](../../../packages/tddy-code-restructuring/docs/path-survey.md) (test-throughout, `defined_at` co-moving), [facades.md](../../../packages/tddy-code-restructuring/docs/facades.md) (no facade for a test module), [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md), [plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md) (prose).
- **`tddy-tools`, `tddy-index-daemon`, `tddy-lsp`**: no change.

## Related Feature Documentation

- [PRD-2026-10-09-reshape-tests-follow.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-tests-follow.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md): `## Rust operations (v1)`, `## Path survey`, `## Known limitations`

## Summary

A cross-crate move takes along every `#[cfg(test)] mod t;` declared beside a moved module (in the declaring file, which stays) whose paths into the origin all reach the moving set. Its files land beside the moved module, its declaration (with attributes and doc comments) moves from the origin to the end of the destination root, its paths are re-pointed, and its crates join `[dev-dependencies]`. A test module that also needs code staying behind stays, with a note naming the path that keeps it. Every file reached through a `#[cfg(test)] mod` is surveyed as test code. The header pass re-points a path that reaches a co-moving module through a re-export into the destination.

## Background

`#carve` 21/21 R9 hand-moved four test files and hand-fixed four paths and one manifest line after a `move_cluster_to_crate` of 46 modules (commits `e4e1f28eb`, `95bf932d0`). One file was a directory child (node 5's). Three were siblings declared in `connection_service.rs`. Their hand fixes show the three gaps this node closes: the engine never looks at sibling test declarations; `reach` re-points `super::recipe_enables_conversation_spawn` (through the parent's `pub(crate) use conversation_spawn::*;`) back at the origin; and an out-of-line test file is surveyed as production code, so `tempfile` would have gone to `[dependencies]`.

## Responsibility

- **Considered declarations.** `test_declarations(text)` reads every top-level `#[cfg(test)]` / `#[cfg(all(test, …))]` `mod t;` (not inline) with the span of its contiguous attributes, doc comments and the `mod` line, and whether one attribute is `#[path …]`. `test_modules` considers those in each member's `declared_in` file when that file is not travelling, and that are not themselves members or carried.
- **Classification** (static, text and the re-export walk only). Each considered module's files (`module_files::files_of`) are surveyed at module path `declared_in`'s path + `t`. Origin-naming paths (`defining_crate == origin`) must all reach the moving set — `travels_with` on `resolved` **or** `defined_at` against members, their carried subtrees and modules moved by earlier operations of the plan — and at least one must. Then it follows; otherwise it stays.
- **Outside references** (resolve-time). A follower whose items `ModuleReferences::outside_references` reports named from a file outside the travelling set stays, with a note.
- **Follow edits.** Renames via `module_files::relocated` to the member's landing directory; `repointed_header_in` per file with `Gate::Test` and tree root `declared_in path + t`; origin declaration span removed; `#[cfg(test)]\nmod t;` with the doc comments appended to the destination root; test files joined to the travelling set **before** the caller survey; their crates into `dev_crates_named`.
- **Test throughout.** A file reached through a `#[cfg(test)] mod` (a sibling follower, or a node 5 carried file at or below a gated declaration) has every surveyed path marked `in_test` before the header pass reads the survey.
- **`reach` reads `defined_at`.** A path whose `defined_at` travels with a co-moving module lands at `crate::<landing>…` like one whose `resolved` does. "Stays inside the module" keys on the file's own tree root, not on the member's home, so a sibling's `super::moved` is rewritten while a test module's `self::helpers` is kept.
- **Static refusal.** A follower whose landing file exists, or whose name the destination root already declares, is a finding in `crate_move::unrunnable` (plain `check`) and refused by `apply` with the same text.
- **Notes.** One per follower and one per staying module that names the move, in `cluster_resolution`'s notes.

### Rules (the contract)

1. **Considered**: `#[cfg(test)]` or `#[cfg(all(test, …))]` directly on a `mod t;` at the top level of a member's `declared_in`, that file not travelling. `#[cfg(any(test, …))]`, `cfg_attr` and a name ending `_tests` without the attribute are not test modules (same marker as `source_scan.rs:7-12`).
2. **Follows** iff (a) every surveyed path of every file of `t` with `defining_crate == origin` reaches the moving set, (b) at least one path reaches it, (c) no `#[path]` on the declaration, (d) no item of `t` is named from outside the travelling set (resolve-time only).
3. **Stays, with a note** when (b) holds and (a), (c) or (d) fails: `` test module `t` stays in `<declared_in>`: it names `<written>` (line N), which stays in `<origin package>` `` / `` …: it is placed with `#[path]` `` / `` …: `<file>` names its `<item>` ``.
4. **Stays, silently** when (b) fails.
5. **Lands**: beside the member it names first in plan order — the member's landing directory (`<dest>/src/` for a member landing at the root). Declared at the end of `<dest>/src/lib.rs`, after one blank line, as the moved span with `pub`/`pub(…)` kept as written (test modules are private by convention).
6. **Refused** (static, nothing written): landing file exists, or `<dest>/src/lib.rs` declares `t`: `` test module `t` follows `<member>` but `<dest>/src/t.rs` already exists — moving it would merge two modules ``.

Example (R9 shape):

```text
origin/src/parent.rs        pub(crate) use a::*; pub(crate) mod a; /// tests a\n#[cfg(test)]\nmod a_tests;
origin/src/parent/a_tests.rs use super::recipe; use super::a::Thing; fn t() { crate::facade::f(); tempfile::tempdir(); }
→ dest/src/a_tests.rs       use crate::a::recipe; use crate::a::Thing; fn t() { other::defined::f(); tempfile::tempdir(); }
→ dest/src/lib.rs           …\n\n/// tests a\n#[cfg(test)]\nmod a_tests;\n
→ dest/Cargo.toml           [dev-dependencies] tempfile = "3"
→ origin/src/parent.rs      the three declaration lines gone
note: test module `a_tests` (1 file(s)) follows `parent::a`: everything it names in `origin` moves
```

## Boundaries

- **No new operation, plan field, flag or wire message**, and no opt-out (F3).
- **No splitting, widening or moving of a staying test module.** A staying module compiles through the origin's facade or the caller re-points the move already makes; anything private it reaches is node 7's (`feature/reshape/move-widen`).
- **No change to node 5's carrying rule.** Children (test or not) of travelling files are node 5's; this node only reads gated ones as test code.
- **No change to the span of a member's removed `mod` line** (node 3's, `feature/reshape/tidy-facades`). The test declaration span is this node's own reader.
- **No `pub(in …)` respelling** inside test files (node 8's).
- **No inline `mod tests { … }` extraction, no test modules declared outside the member's declaring file, no same-crate operation** (proposed todos).
- **No growth** of `resolve_cluster`, `sightings`, `items_of_module`, `stranded_siblings`, `resolve_opening`. **No `cluster.rs` growth beyond a call**; logic and tests go in new files.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| 5 `move-children` (`feature/reshape/move-children`) | `Move::carried` / `module_files::MovedFile` / `module_files::relocated`; the travelling set over carried files; `cluster::member_changes`; `header::repointed_header_at`; `crate_move::cluster_resolution` with notes; static uncarriable findings in `unrunnable` | Followers are landed with `relocated`; the moving set for classification is members ∪ their carried files; follower files are added to node 5's travelling set; `repointed_header_at` delegates to the new `repointed_header_in`; notes are appended to `cluster_resolution`'s; the refusal joins `unrunnable` beside node 5's findings | Re-implement carrying, relocation, facade visibility, restricted-child refusal or the `..` fix; change which children node 5 carries |

Refined at the contract commit: node 5's surface is all `todo!()` until its green, so tests 9, 10, 11 and 15 (they read `cluster_resolution`'s notes) panic in node 5's `crate_move::cluster_resolution` until node 5 is green, and test 8 needs node 5's carrying before it can show the test gate. The fake reference set node 5 kept local to `tests/crate_move_children.rs` is now shared as `tests/harness/known_references.rs` (F5); node 5's local copy is left for its owner to switch over. Fake reference sets must set node 7's `declared_at`/`within`/`kind`, and `MovingCluster` literals node 9's `creates: None`.

Textual collisions (not edges): node 3 (`facade_writer`, declaration spans), node 7 (`cluster.rs`), node 8 (`header.rs`, `check_precondition_parity.rs`), node 13 (`backends/rust.rs` untouched here). Rebase over them.

## Draft PR contract

The wave-2 contract commit (the first push of this PR, never its deliverable) publishes this **owned surface, new today**. Everything is crate-private:

- `crate_move::source_scan::TestDeclaration { name: String, span: Range<usize>, placed_by_path: bool }` and `source_scan::test_declarations(text: &str) -> Vec<TestDeclaration>`.
- `crate_move::header::Gate { AsWritten, Test }`, `header::FileInTree { module_path: Vec<String>, tree_root: Vec<String>, gate: Gate }`, and `header::repointed_header_in(workspace: &Workspace<'_>, text: &str, moving: &Move, file: &FileInTree, co_moving: &BTreeSet<String>) -> Result<Header>`. Node 5's `repointed_header_at` delegates to it.
- `crate_move::test_modules`:
  - `FollowingTest { declared_in: String, declaration: TestDeclaration, module_path: Vec<String>, files: Vec<MovedFile>, follows: String }`
  - `TestModules { following: Vec<FollowingTest>, notes: Vec<String> }`
  - `sorted(workspace: &Workspace<'_>, members: &[Move], earlier: &BTreeSet<String>) -> Result<TestModules>`
  - `kept_by_outside_references(engine: &mut dyn ModuleReferences, workspace: &Workspace<'_>, modules: TestModules, travelling: &BTreeSet<String>) -> Result<TestModules>`
  - `travelling(members: &[Move], carried: &[MovedFile], following: &[FollowingTest]) -> BTreeSet<String>`
  - `follow_changes(workspace: &Workspace<'_>, members: &[Move], following: &[FollowingTest], co_moving: &BTreeSet<String>) -> Result<(Vec<FileEdit>, BTreeSet<String>)>` (edits, dev crates)
  - `test_gated(workspace: &Workspace<'_>, carried: &[MovedFile]) -> Result<BTreeSet<String>>`
  - `unfollowable(workspace: &Workspace<'_>, ops: &[RefactorOp], index: usize, op: &RefactorOp) -> Result<Vec<String>>`
- **Failing tests:** acceptance tests 1–11, 13–20, 20a and 22 below. Items 12 and 21 are green pins. Shared test helpers added: `tests/harness/known_references.rs` (`AKnownReferenceSet`, `nothing_reaches_anything`, `position_of`) and `harness::a_workspace_whose_module_has_a_sibling_test_module` + `A_SIBLING_TEST_MODULE`.

## Green wave

**Wave:** 2 of 4.
**Greenable independently:** no — after `feature/reshape/move-children` (K=5) is green; it does not need any other wave-1 node.
**Concurrent with:** `feature/reshape/move-impl-members`, `oversized-files`, `fn-sizes-rest` (textual collisions only).
**Blocks:** nothing in this stack.
**Real edges (whole stack):** `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`, `17→18`, `6→18`, `4→19`, `17→19`.

## Successor PRs

None in `#reshape`: no node consumes this behaviour. The second stack (crate split of `tddy-code-restructuring`) uses it for `runner/tidy.rs`'s test module and the daemon-side carves.

## Scope

- [ ] Declaration reader with spans, `#[path]` detection
- [ ] Test-throughout gate for followers and node 5's gated children
- [ ] `reach` reads `defined_at` and the file's tree root
- [ ] Classification, earlier-operation case, outside references
- [ ] Follow edits, travelling set, destination declaration, dev-dependencies
- [ ] Static refusal with check/apply parity
- [ ] Notes and count parity
- [ ] Compiled live case
- [ ] Docs at wrap; claimed todo deleted

## Technical changes

### State A

- Sibling test declarations are never read on a crate-move path: `left_behind` edits members' `mod` lines only (`facade_writer.rs:31-104`); `declared_in_destination` writes `pub mod` for members only (`facade_writer.rs:291-313`).
- The travelling set is member sources (`cluster.rs:118`; node 5: plus carried files), so a sibling test's references to moved items are caller rewrites (`crate_move.rs:221-251`).
- `in_test` comes from attributes inside the scanned text only (`sighting_walk.rs:44-115`); an out-of-line test file is production code to the header pass (`header.rs:101-105`) and the cycle refusal.
- `reach` asks `travels_with` of `resolved` only (`header.rs:142-144`); the body precondition asks it of `defined_at` (`preconditions.rs:103-106`, `:126-137`). `stays_inside_the_module` keys on `moving.home.path` (`header.rs:147-148`).
- `ChildModule` records no attribute (`module_items.rs:12-20`); `Scan::cfg_test_attribute` exists (`source_scan.rs:85-87`).

### State B

- `test_declarations` reads gated declarations and their spans; `test_modules::sorted` classifies; `follow_changes` writes renames, headers, the origin removal and the destination declaration; `cluster_resolution` carries their notes.
- `repointed_header_in` surveys at an explicit module path, marks every path `in_test` under `Gate::Test`, and passes the tree root to `reach`.
- `reach` treats `defined_at` inside the moving set as co-moving.
- `unrunnable` reports the landing collision.

### Delta

- New `crate_move/test_modules.rs` (~220 lines), `crate_move/source_scan/test_declarations.rs` (~80 lines).
- `header.rs`: the body of `repointed_header_at` becomes `repointed_header_in`; `reach` gains the tree-root argument and the `defined_at` check (~15 lines).
- `cluster.rs`: `travelling` via `test_modules::travelling`; one `follow_changes` absorb; `member_changes` passes `Gate::Test` for `test_gated` files (net ≤ +5 lines, `resolve_cluster` not longer than today).
- `preconditions.rs`: one call in `unrunnable`.

## Implementation milestones

- [ ] **M1** `test_declarations` + unit tests 19–20
- [ ] **M2** `repointed_header_in`, `Gate::Test`, `test_gated`; tests 7, 8
- [ ] **M3** `reach` on `defined_at` and tree root; tests 5, 16
- [ ] **M4** `sorted`, `kept_by_outside_references`; tests 9–13
- [ ] **M5** `follow_changes`, `travelling`; tests 1–4, 6, 14
- [ ] **M6** `unfollowable`, notes; tests 15, 17, 18
- [ ] **M7** live case 22
- [ ] **M8** docs at wrap

## Testing plan

### Testing Strategy

**Primary: library level, no rust-analyzer.** Through the public `crate_move::cluster_resolution` / `resolve_cluster` / `unrunnable_moves`, with node 5's fake `ModuleReferences` over tempdir workspaces (F5). Exact text and notes.

**Thin live layer:** one case in `tests/move_module_to_crate_acceptance.rs` (already in `.config/rust-e2e.filterset` and the `rust-analyzer` group), oracle `assert_compiles_with_its_tests`.

Fixture for `tests/crate_move_test_modules.rs`: `origin/src/lib.rs` declares `pub mod parent; pub mod facade;` (`facade` = `pub use other::defined;`); `origin/src/parent.rs` declares `pub(crate) mod a; pub(crate) use a::*; pub mod keeper;` and, each with a doc comment, `#[cfg(test)] mod a_tests;` (names only `super::recipe`, `super::a::Thing`, `crate::facade::defined::f`, `tempfile`), `#[cfg(test)] mod mixed_tests;` (`use super::*;` + `super::a::Thing`), `#[cfg(test)] mod unrelated_tests;` (names `std` only); `origin/Cargo.toml` declares `other` and dev-dependency `tempfile`; destination `dest` empty.

### Coverage Requirements

- [ ] Follows: sibling of a nested member, of a top-level member, with its own children, `reexport` glob and none
- [ ] Paths: `super::` to the moved module, `super::` through the parent's glob, `crate::` through a facade, `self::` inside the test module kept
- [ ] Manifest: dev-dependencies for followers and gated carried children
- [ ] Stays: mixed, unrelated, `#[path]`, referenced from outside
- [ ] Multi-operation plan
- [ ] Refusal parity; notes; count parity
- [ ] Pins: non-gated `*_tests` module is not considered; existing suites untouched

## Acceptance tests

Unless marked *green pin*, each test is **red on `master`** (and on node 5's tip) for the reason in its group heading.

### `packages/tddy-code-restructuring/tests/crate_move_test_modules.rs` (new; library level, fake reference set)

Red because no crate-move code reads a sibling test declaration, `reach` ignores `defined_at`, and out-of-line test files are production code.

1. `a_sibling_test_module_naming_only_moved_code_moves_beside_it_in_the_same_edit`: rename `origin/src/parent/a_tests.rs` → `dest/src/a_tests.rs`.
2. `the_origin_loses_the_test_declaration_with_its_attribute_and_doc_comment`
3. `the_destination_root_declares_the_test_module_under_cfg_test_after_its_last_line`
4. `a_following_test_modules_super_path_to_the_moved_module_becomes_a_crate_path`
5. `a_super_path_through_the_parents_glob_of_a_moved_module_is_re_pointed_into_the_destination`: `super::recipe` → `crate::a::recipe`.
6. `a_crate_path_through_an_origin_facade_is_re_pointed_to_the_defining_crate`: `crate::facade::defined::f` → `other::defined::f`.
7. `the_crates_a_following_test_module_names_join_dev_dependencies`: `tempfile` under `[dev-dependencies]`, not `[dependencies]`.
8. `a_child_carried_under_a_cfg_test_declaration_sends_its_crates_to_dev_dependencies`: node 5's `a/a_inner_tests.rs` shape.
9. `a_test_module_that_also_names_code_staying_behind_stays_and_the_resolution_notes_why`: `mixed_tests`, note names `super::*`.
10. `a_test_module_whose_items_another_file_names_stays_and_the_resolution_notes_why`
11. `a_test_module_placed_with_a_path_attribute_stays_and_the_resolution_notes_why`
12. `a_test_module_naming_nothing_the_move_takes_is_left_alone`: *green pin* (no rename, declaration kept). The "without a note" half is not asserted: reading notes needs node 5's `cluster_resolution`.
13. `a_test_module_of_two_modules_moved_by_two_operations_follows_the_second`
14. `no_reference_inside_a_following_test_module_is_rewritten_as_a_caller`: `reexport: none`.
15. `the_resolution_notes_each_test_module_that_follows`
16. `a_moved_member_reaching_a_co_moving_sibling_through_its_parents_glob_is_re_pointed_into_the_destination`

### `packages/tddy-code-restructuring/tests/check_precondition_parity.rs` (existing; new tests appended)

Red because no static pass reads a sibling test declaration.

17. `a_static_check_reports_a_following_test_module_whose_target_already_exists_in_the_destination`

Test 18 needs a reference engine, which this file does not carry, so it lives in `tests/crate_move_test_modules.rs`:

18. `check_and_apply_refuse_a_following_test_modules_merge_with_the_same_message`

### Unit tests, `packages/tddy-code-restructuring/src/crate_move/source_scan/test_declarations.rs`

19. `reads_a_cfg_test_declaration_with_its_doc_comment_and_attributes_as_one_span`
20. `a_cfg_any_test_or_an_inline_test_module_is_not_a_test_declaration`
20a. `a_test_declaration_placed_with_a_path_attribute_says_so` (surface unit test)

### Existing suites, unchanged (*green pins*)

21. `tests/crate_move_children.rs`, `tests/cluster_move.rs`, `tests/move_facades_acceptance.rs`, `tests/move_paths_acceptance.rs`, `tests/nested_module_move_acceptance.rs` pass without edits; a module named `*_tests` without `#[cfg(test)]` is not considered.

### `packages/tddy-code-restructuring/tests/move_module_to_crate_acceptance.rs` (existing, live; already registered)

A cluster of one module is refused (`move_cluster_to_crate needs also`), so the single-module case lives with the single-module suite, beside node 5's `files_reported` helper.

22. `moves_a_module_with_its_sibling_test_module_and_every_crate_compiles_with_its_tests`: fixture `harness::a_workspace_whose_module_has_a_sibling_test_module` (`A_SIBLING_TEST_MODULE`); also asserts dry-run and apply `-> N file(s)` are equal and the follow note is printed. Red: the test module stays in the origin (it still compiles there through the glob facade) and no note names it.

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by this plan:
- A staying test module is a note, not a finding: R9's `stack_child_spawn_tests` was correct to stay.
- The destination declaration goes at the end of the root, not in sorted position: the repository's convention keeps `#[cfg(test)] mod` lines after production declarations (`tddy-agent-launch/src/lib.rs:49-54`).

Taken by the developer (2026-10-09, PRD review), with all recommendations accepted:

- **F1: who owns "test code throughout" for node 5's carried children.** Node 5's PRD reads `#[cfg(test)]` in-file only, so a carried `claude_cli_spawn_steps_tests.rs` would send `tempfile` to `[dependencies]` and could trip the cycle refusal. **Decided (2026-10-09): this node owns it for both** (one rule, one place: `Gate::Test` + `test_gated`), and node 5 stays as approved.
- **F2: what counts as "testing only moved code".** Every origin-naming path reaches the move, and at least one does. **Decided (2026-10-09) as stated**; the alternative (only `super::` paths, as the todo phrased it) would move a test that also names `crate::test_util` from the origin and break it.
- **F3: no opt-out field.** **Decided (2026-10-09): none**: a follower by construction needs nothing that stays; add a field only when a plan needs one.
- **F4: outside references keep a test module behind** (resolve-time, so plain `check` cannot see it). **Decided (2026-10-09): keep it** (rare, but moving a module whose helpers another test imports breaks that test); the parity gap is stated in the limitations.
- **F5: test fixtures.** **Decided (2026-10-09): move node 5's fake reference set into `tests/harness/`** (a test-only move) if node 5 kept it local to `tests/crate_move_children.rs`, rather than a second copy.

**OPEN:** none.

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

**Contract commit (wave 2, 2026-10-09)** — scoped runs only; `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings` and `cargo fmt --all --check` clean.

| Test | File | State | Why |
|---|---|---|---|
| 1 sibling moves | `tests/crate_move_test_modules.rs` | 🔴 red | only `a.rs` renamed |
| 2 origin loses declaration | same | 🔴 red | `parent.rs` still declares `a_tests` |
| 3 destination declares it | same | 🔴 red | root reads `pub mod a;` only |
| 4, 5, 6 paths | same | 🔴 red | `a_tests` is not moved or re-pointed |
| 7 dev-dependencies | same | 🔴 red | destination manifest gains no `tempfile` |
| 8 gated carried child | same | 🔴 red | nothing carried yet (node 5) and no test gate |
| 9, 10, 11, 15 notes | same | 🔴 red | panic in node 5's `crate_move::cluster_resolution` `todo!()` (`crate_move.rs:251`) — red until node 5 is green, then on this node's notes |
| 12 unrelated left alone | same | 🟢 green pin | |
| 13 two operations | same | 🔴 red | `ab_tests` not renamed at the second operation |
| 14 no caller rewrite | same | 🔴 red | `a_tests` gains `use destination::a::Thing;` as a caller |
| 16 member through parent glob | same | 🔴 red | resolve refuses as a cycle: `b.rs still names origin (origin::parent::a::recipe)` |
| 17 static merge finding | `tests/check_precondition_parity.rs` | 🔴 red | no finding |
| 18 check/apply same message | `tests/crate_move_test_modules.rs` | 🔴 red | apply `Ok(())`, check `[]` |
| 19, 20, 20a declaration reader | `src/crate_move/source_scan/test_declarations.rs` | 🔴 red | `todo!("test_declarations")` |
| 22 live | `tests/move_module_to_crate_acceptance.rs` | 🔴 red | dry run `5 file(s)`, no follow note, test file stays |

Test 13's fixture gives `b` no `super::recipe`: through an earlier operation's facade, the re-export walk cannot follow `pub use a::*;` once `a` is `pub use destination::a;` (a `use`-bound name, not a child), which refuses `b` as a cycle — a separate gap, reported to the orchestrator as a proposed todo.

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-tests-follow-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-tests-follow.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point, not edited while planning)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests (contract surface + its unit tests)
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the scoped tests (`./test -p tddy-code-restructuring`) and verify 100% pass; CI answers for the rest of the workspace
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
- [ ] Linting and formatting (`cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) when the PR is set ready for review; deletes `2026-10-09-reshape-tests-follow-initial-discovery.md` and the claimed todo
- [ ] USER REVIEW — work complete, decide next steps
