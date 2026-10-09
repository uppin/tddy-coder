# Changeset: cross-crate moves carry a module's directory children and keep its `mod` visibility

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Feature (engine capability for `move_module_to_crate` / `move_cluster_to_crate`) plus defect fixes
**Stack**: `#reshape` 5/19, branch `feature/reshape/move-children`, green wave 1. PR title:
`feat(code-restructuring): crate moves carry a module's children and keep its mod visibility (#reshape 5/19)`.
**Base branch** in the linear stack: `feature/reshape/extract-method-clean` (K=4). That is a line position only, not a dependency.
**PR:** [#602](https://github.com/uppin/tddy-coder/pull/602) (draft), lands after #601.
**Real edges:** none in, one out. `move-children → tests-follow` (5→14): node 14 extends this node's carried-file set with sibling `#[cfg(test)]` modules.

## Initial Discovery

The codebase exploration that grounded this plan is in [initial-discovery.md](./2026-10-09-reshape-move-children-initial-discovery.md):

- Exploration 1 is the whole-work discovery.
- Exploration 2 is this node's, with every claim verified at file:line against `4a5c42b1b`.

State A below is distilled from that file. Grep traces are not repeated here.

## Prerequisites

`grep -rl 'Claimed by:'` over `packages/tddy-code-restructuring/docs/code-issues/` finds no claimed issue in the path, so there is no wait-or-proceed fork.

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md](../todo/2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md) | ✅ **RESOLVED HERE**, **narrowed at wrap** | Acceptance items 1 (children move), 2 (count parity, pinned plus a note) and 4 (`check` reports what cannot be carried) are done here. **Item 3** (`libc`, from `[target.'cfg(unix)'.dependencies]`) is node 9's slice, per the developer decision of 2026-10-09. The stranded doc comment is node 3's span work. At wrap the file is narrowed to whatever of those two has not landed; it is deleted only if both have |
| [2026-10-08-restructure-module-move-strands-its-directory-child-and-the-cluster-leaves-a-dangling-self-reexport.md](../todo/2026-10-08-restructure-module-move-strands-its-directory-child-and-the-cluster-leaves-a-dangling-self-reexport.md) | ✅ **RESOLVED HERE** | Children carried at their nested position; no self facade. Deleted at wrap |
| [2026-10-08-restructure-move-cluster-ignores-also-members-that-are-directory-children-and-their-crates.md](../todo/2026-10-08-restructure-move-cluster-ignores-also-members-that-are-directory-children-and-their-crates.md) | ✅ **RESOLVED HERE**, **narrowed at wrap** | `also` children folded and nested; crates only children name reach the manifest. The **create-crate** step, `async-trait` (named only by an attribute) and `libc` (target table) remain for node 9. Narrowed at wrap to those three, or deleted if node 9 has landed |
| [2026-10-08-restructure-move-to-crate-does-not-read-a-restricted-mod-declaration.md](../todo/2026-10-08-restructure-move-to-crate-does-not-read-a-restricted-mod-declaration.md) | ✅ **RESOLVED HERE** | `module_declaration` reads any visibility. Deleted at wrap |
| [2026-10-08-hand-widened-mod-declarations-before-engine-moves.md](../todo/2026-10-08-hand-widened-mod-declarations-before-engine-moves.md) | ✅ **RESOLVED HERE** | Restricted declarations are moved and the facade keeps their visibility, so no pre-move widening commit is needed. Deleted at wrap together with its sibling above |
| [2026-10-08-restructure-cluster-move-misses-body-paths-and-writes-self-referencing-test-reexports.md](../todo/2026-10-08-restructure-cluster-move-misses-body-paths-and-writes-self-referencing-test-reexports.md) | ✅ **RESOLVED HERE** | Item 1 (self re-exports) and item 2 (`..crate::…` struct update) are fixed here. Item 3 is only a pointer to node 7's entry. Deleted at wrap |
| [2026-09-09-macro-expansion-as-a-restructure-operation.md](../todo/2026-09-09-macro-expansion-as-a-restructure-operation.md) | ✅ **RESOLVED HERE**, closed as already done | A research note whose recommendation is "do not build". Nothing to implement. Deleted at wrap (developer-approved) |
| [2026-09-09-restructure-defects-from-the-connection-service-split.md](../todo/2026-09-09-restructure-defects-from-the-connection-service-split.md) | ✅ **RESOLVED HERE**, closed as already done | D6–D10 are fixed on `master`. The facade caveat survives as item M of `2026-09-24-restructure-apply-gaps-…` (node 2's file). Deleted at wrap (developer-approved) |
| [2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md](../todo/2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md) | — Unrelated (node 8 claims it) | This node makes the finding read **every carried file**. It still reads each file's header only. Node 8 widens header → all paths. Both edit `paths_naming_the_origin`: a textual collision, not a behavioural one |
| [2026-10-08-restructure-move-to-crate-misses-a-crate-named-only-by-an-attribute-macro.md](../todo/2026-10-08-restructure-move-to-crate-misses-a-crate-named-only-by-an-attribute-macro.md) | — Unrelated (node 9) | Not touched |
| [oversized-file-backends-rust.md](../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md) | ⚠ **DURING** | Line-neutral: the two crate-move arms in `resolve_opening` swap `Resolution::of(crate_move::resolve…(…)?)` for `crate_move::resolution…(…)?`. No new line in `backends/rust.rs` |
| Function-size list (whole-work discovery, Exploration 3): `resolve_cluster` (66, `cluster.rs:111`), `stranded_siblings` (70, `cluster/stranded.rs:54`), `sightings` (95, `source_scan/sighting_walk.rs:33`), `resolve_opening` (163, `backends/rust.rs:1245`) | ⚠ **DURING** | None of these grows. New logic goes in new functions; `resolve_cluster`'s loop body moves into a new `member_changes`, so it **shrinks**. `sightings` and `stranded_siblings` are not edited (the `..` fix is in `Scan::continues_a_path`; carried files are read in `paths_naming_the_origin`) |
| `packages/tddy-code-restructuring/docs/code-issues/*` others | — | Not in the path |

## Affected Packages

**`tddy-code-restructuring`**
- **New:** `src/crate_move/carried.rs`, with the carried-file set, `also` folding, notes and the uncarriable findings.
- **Edited:**
  - `src/crate_move.rs`: `mod carried;`, `resolution`/`cluster_resolution`, and `FacadeEntry` + `facade_lines_for_plan`.
  - `src/crate_move/module_files.rs`: `MovedFile`, `relocated`.
  - `src/crate_move/moving.rs`: `Move::carried`, `Move::declaration`.
  - `src/crate_move/moving/facade_writer.rs`: skip travelling declaring files, visibility-preserving facades, `WrittenFacade.visibility`.
  - `src/crate_move/cluster.rs`: `member_changes`, a travelling set over carried files, folding.
  - `src/crate_move/cluster/stranded.rs`: `paths_naming_the_origin` reads carried files.
  - `src/crate_move/preconditions.rs`: carried bodies, uncarriable findings.
  - `src/crate_move/header.rs`: `repointed_header_at`.
  - `src/crate_move/manifest_edits.rs`: `ModuleDeclaration`, `declared_module`, a visibility-tolerant `module_declaration`/`declared_module_name`.
  - `src/crate_move/source_scan.rs`: `continues_a_path`.
  - `src/backends/rust/module_reparent/relocation.rs`: `plan` delegates to `module_files::relocated`.
  - `src/backends/rust.rs`: two line-neutral call swaps.
- **Docs at wrap:**
  - [path-survey.md](../../../packages/tddy-code-restructuring/docs/path-survey.md) (`..` paths, carried files)
  - [facades.md](../../../packages/tddy-code-restructuring/docs/facades.md) (visibility mirrors the declaration)
  - [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) (`## Rust operations (v1)` rows 336–337, `## Path survey`, `## Known limitations`)
  - [plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md) (prose only)

**`tddy-tools`, `tddy-index-daemon`, `tddy-daemon-rpc`:** no source change.

## Related Feature Documentation

- [PRD-2026-10-09-reshape-move-children.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-move-children.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md): `## Rust operations (v1)`, `## Path survey`, `## Known limitations`

## Summary

A cross-crate move now carries a module's **directory children**: every file its `mod` declarations lead to, at the same relative place under the destination. Each child goes through the same path survey, header re-point, manifest pass, caller survey and refusals as the module's own file.

- An `also` member that is a child of another member is folded into it.
- A `pub(crate)`, `pub(super)` or `pub(in …)` `mod` line is read, and the facade left in its place keeps that visibility.
- No file arriving in the destination gets a facade naming the destination.
- A body path after `..`/`..=` is surveyed.
- Plain `check` reports every child the move cannot carry.

## Background

`#live-plan` 12/15 and `#carve` 21/21 R1/R3/R4/R7/R8 each needed a hand `git mv`, hand manifest lines, a hand-deleted self re-export, a hand re-point of `..crate::…`, or a pre-move widening of `pub(crate) mod`. In every case `check --deep` and `--dry-run` were green, and only the compile gate failed.

The causes:
- the move renames exactly one file per member (`cluster.rs:133`);
- nested members are deliberately flattened (`header.rs:47`, `moving.rs:87`);
- `module_declaration` strips only `"pub "` (`manifest_edits.rs:13`);
- the tokenizer reads `..crate` as a field access (`source_scan.rs:137-143`).

The walker the move needs (`module_files::files_of`) and the relocation rule (`reparent_module`'s `relocation::plan`) already exist and are tested.

## Responsibility

- **Carried files.** A moved module's file set is `files_of(<its file>)`. Each file moves by `git mv` to the same place relative to the module under `<dest>/src/`. Each gets `repointed_header_at` with its module path below the module. The crates it names join the destination manifest. It joins the travelling set, so references from inside the tree are not callers. Its outside references are surveyed and re-pointed when `reexport: none`.
- **`also` folding.** A member whose module path lies under another member's is dropped from the member list, since its parent carries it. It is accepted, not refused.
- **No self facade.** `left_behind` emits nothing for a member whose `declared_in` is a travelling file. The parent's `mod child;` and any `pub use child::…` stay as written. `declared_in_destination` declares only real members.
- **Uncarriable children,** as findings in `crate_move::unrunnable`, so plain `check` reports them:
  - a `mod x;` that leads to no file;
  - a carried file with a `#[path]` attribute on a `mod`;
  - a target that already exists in the destination;
  - a carried child that another operation of the plan anchors, whether separately or to another destination;
  - a carried child's body reaching a module that stays behind (`stays_behind_through_a_body` over every carried file).
- **Restricted child reached from outside** (F3, approved: refuse). Resolve-time, so `check --deep` and `apply` report it: a caller outside the tree whose written path passes through a carried child declared `pub(crate)`/`pub(super)`/`pub(in …)`/private.
- **Restricted declarations.** `manifest_edits::declared_module` returns the span **and** the visibility text of `[<vis>] mod <name>;` for any visibility. `module_declaration` and `declared_module_name` use it.
- **Facade visibility** (F2, approved: mirror exactly). The glob facade is written with the declaration's visibility (`pub(crate) use dest::x;`; private → `use dest::x;`). Lines are grouped per (destination, visibility). An earlier line of the same visibility is extended.
- **`..` paths.** `Scan::continues_a_path` treats an identifier behind `..` as opening a path.
- **Notes.** `crate_move::resolution` / `cluster_resolution` return a `Resolution` whose `notes` carry "`{n}` file(s) move with `{module}`, with the directory of its children" for each member that carries more than its own file.
- **Shared relocation** (F4, approved). `module_files::relocated` is the one rule; `reparent_module`'s `relocation::plan` delegates to it.

## Boundaries

- **No new operation, plan field, flag or wire message.** The plan line is unchanged.
- **No widening of anything** that the origin reaches: items, fields, methods, or a restricted child. Restricted children are refused (F3); everything else is node 7's (`feature/reshape/move-widen`).
- **No `pub(in …)` respelling inside moved or carried files.** That is node 8's (`feature/reshape/move-grouped-use`), per the developer decision of 2026-10-09. This node reads `pub(in …)` only on the declaring file's `mod` line, which stays in the origin, and mirrors it verbatim in the origin's facade.
- **No change to what a removed declaration's span covers** (doc comments, attributes). That is node 3's (`feature/reshape/tidy-facades`). Node 3 extends `module_declaration`'s span; this node changes only which lines match. See Dependencies.
- **No manifest table beyond `[dependencies]`/`[dev-dependencies]`, and no create-crate.** Those are node 9's (`feature/reshape/new-crate`).
- **No grouped-`use` splitting.** Node 8's.
- **No sibling test modules** (`#[cfg(test)] mod x_tests;` beside the module, not under it). Node 14 extends the carried set.
- **No change to a lone nested member.** Its parent stays, so it still lands at the destination root (`header.rs:47`).
- **No growth of the functions on nodes 16/19's list:** `resolve_cluster`, `stranded_siblings`, `sightings`, `items_of_module`, `resolve_opening`, `check_plan`.
- **No `cluster.rs` growth beyond `member_changes`.** New logic goes in `crate_move/carried.rs`, and new tests go in new test binaries, because `cluster.rs` is 921 lines.

## Dependencies

This node has no parent: it consumes no behaviour of any other `#reshape` node and is greenable on `master` as it stands.

Textual collisions while wave 1 runs concurrently (not edges; each side rebases over the other):

| Node | Shared file / symbol | How the two edits compose |
|---|---|---|
| 3 `tidy-facades` | `manifest_edits::module_declaration`; `facade_writer::leaving`; `declared_in_destination` | Node 3 widens the returned **span** upward to the doc comments and refuses other attributes. This node changes the **match** to accept any visibility, through the new `declared_module`. Whoever lands second keeps both: match by `declared_module`, span extended by node 3's trivia rule. In `declared_in_destination` node 3 writes the moved doc above `pub mod`; this node skips folded children. The two are disjoint |
| 7 `move-widen` | `facade_writer.rs`, `cluster.rs` | Node 7 widens what the origin reaches. This node does not widen; the restricted-child refusal stays until a follow-up converts it |
| 8 `move-grouped-use` | `header.rs`, `cluster/stranded.rs` `paths_naming_the_origin`, `check_precondition_parity.rs` | Node 8 widens header → all paths and rewrites the pinned test at `:497`. This node iterates the same function over carried files and adds new tests at the end of the file |
| 9 `new-crate` | `moving.rs` `destination_manifest`, `manifest_edits.rs` | Node 9 reads target tables and creates crates. This node only feeds more `crates_named` from carried files |
| 1, 2, 4, 6, 10–12 | `crate_move/*`, `backends/rust.rs` | File overlap only |

## Draft PR contract

The wave-2 contract commit (the first push of this PR, never its deliverable) publishes this **owned surface, new today**. Everything is crate-private unless marked `pub`:

- `crate_move::module_files::MovedFile { from: String, to: String, below: Vec<String> }` and `module_files::relocated(old: &Path, new: &Path, name: &str, files: &[String]) -> Vec<MovedFile>`. `relocation::plan` becomes a call to it.
- `crate_move::manifest_edits::ModuleDeclaration { span: Range<usize>, visibility: String }` and `manifest_edits::declared_module(text: &str, module: &str) -> Option<ModuleDeclaration>`. `module_declaration` keeps its signature.
- `crate_move::moving::Move::carried(&self, workspace: &Workspace<'_>) -> Result<Vec<MovedFile>>`, with the own file first. `Move::declaration(&self, workspace: &Workspace<'_>) -> Result<ModuleDeclaration>`.
- `crate_move::carried`:
  - `fold_carried_members(members: Vec<ModuleHome>) -> Vec<ModuleHome>`
  - `uncarriable(workspace: &Workspace<'_>, ops: &[RefactorOp], index: usize, op: &RefactorOp) -> Result<Vec<String>>`
  - `carried_notes(workspace: &Workspace<'_>, cluster: &MovingCluster) -> Result<Vec<String>>`
  - `restricted_children_reached(workspace: &Workspace<'_>, member: &Move, rewrites: &[PlannedRewrite]) -> Result<()>`
- `crate_move::header::repointed_header_at(workspace: &Workspace<'_>, text: &str, moving: &Move, below: &[String], co_moving: &BTreeSet<String>) -> Result<Header>`. `repointed_header` delegates with `&[]`.
- `crate_move::cluster::member_changes(...)`: private; the loop body of `resolve_cluster`.
- **`pub`:**
  - `crate_move::resolution(engine: &mut dyn ModuleReferences, workspace: &Workspace<'_>, op: &RefactorOp) -> Result<Resolution>`
  - `crate_move::cluster_resolution(engine: &mut dyn ModuleReferences, workspace: &Workspace<'_>, cluster: &MovingCluster) -> Result<Resolution>`
  - `resolve`/`resolve_cluster` keep their signatures and semantics, so existing callers and tests are untouched.
- `crate_move::FacadeEntry { destination: Destination, visibility: String, module: String }`. `facade_lines_for_plan(moved: &[FacadeEntry]) -> Vec<String>` replaces the `(Destination, String)` tuple.
- `facade_writer::written_facade(text, extern_name, visibility: &str, destination_root)`: it takes the visibility to match as a parameter, and matches `"{visibility} use {extern}::"` once implemented. `WrittenFacade` gains **no** field (changed from the plan; see "As published" below).
- **Failing tests:** see `## Validation Results`.

**As published in commit 2 (what differs from the list above):**
- **Two pure functions are already implemented,** because their signatures changed under callers that must keep compiling:
  - `facade_lines_for_plan(&[FacadeEntry])` groups per (destination, visibility) and writes `"{vis} use …"` (`"use …"` when private). Its callers (`leaving`, `extended_facade`) still pass `"pub"` behind a `TODO(reshape-move-children)`, so behaviour is unchanged until green.
  - `written_facade` takes `visibility` and ignores it (`TODO`), still matching `pub use`.
- **Not yet created**, because they have no caller and would only add dead code: `cluster::member_changes` (private) and the `relocation::plan` delegation. Green adds both together with the wiring.
- **The folding moves.** `also` folding was planned for `named_by`. The acceptance tests drive it through the public `resolve_cluster`, so green folds in `read_members`, which serves both the plan path and the API path. `fold_carried_members` keeps its signature.
- **Unwired surface.** Every new crate-private stub carries `#[allow(dead_code, reason = "TODO(reshape-move-children) …")]`, and green removes each one when it wires the stub. `resolution`/`cluster_resolution` are `pub` stubs (`todo!()`) and are not yet called from `backends/rust.rs`.

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes, on `master` as it stands.
**Concurrent with:** `feature/reshape/widen-same-crate`, `multi-seam-extract`, `tidy-facades`, `extract-method-clean`, `methods-leave-type`, `move-widen`, `move-grouped-use`, `new-crate`, `apply-robust`, `move-item-paths`, `anchors-outline`. These are textual collisions only (see Dependencies).
**Blocks:** `feature/reshape/tests-follow` (K=14).
**Real edges (whole stack):** `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`, `17→18`, `6→18`, `4→19`, `17→19`.

## Successor PRs

- `feature/reshape/tests-follow`: consumes `Move::carried` / `module_files::relocated` and the carried travelling set, adding sibling `#[cfg(test)]` modules that test only moved code.

## Scope

- [ ] **Restricted declarations**: `declared_module`, `module_declaration`, `declared_module_name`; merge check
- [ ] **`..` paths**: `continues_a_path`
- [ ] **Carried files**: `relocated`, `Move::carried`, renames, `repointed_header_at`, manifest, travelling set, callers
- [ ] **`also` folding and no self facade**
- [ ] **Refusals**: uncarriable findings (static), restricted child reached (resolve-time), carried bodies, cycle refusal and stranded finding over carried files
- [ ] **Facade visibility**: `FacadeEntry`, grouping, `WrittenFacade.visibility`
- [ ] **Notes and count parity**: `resolution`, `cluster_resolution`, `carried_notes`, line-neutral wiring in `rust.rs`
- [ ] **Registration**: none needed. The two live binaries touched are already in `.config/rust-e2e.filterset` (lines 46, 62) and the `rust-analyzer` group. The new binaries are server-free
- [ ] **Package documentation** at wrap (list under Affected Packages)
- [ ] **Testing**: acceptance tests pass; `./test -p tddy-code-restructuring` scoped; CI for the rest
- [ ] **Code quality**: `cargo check -p tddy-code-restructuring --all-targets`, `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`, `cargo fmt`; no listed function grows; every touched file stays ≤ 500 production lines

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- `resolve_cluster` (`crate_move/cluster.rs:111-176`) emits one `Rename { member.source → member.moved_to() }` per member (`:133`). Its header, caller survey and cycle refusal read `member.source` alone (`:127-131`). `travelling` is the member files (`:118`).
- `Move::of` (`moving.rs:66-84`) gives one source file. `moved_to` (`:87-89`) is `<dest>/src/<last segment>.rs`.
- `declared_in_destination` (`facade_writer.rs:291-313`) declares every member at the destination root.
- `left_behind`/`leaving` (`facade_writer.rs:31-104`) replace each member's `mod` line in `declared_in` with a facade, even when `declared_in` travels. `parent_reexport_edits` (`:152-166`) writes `<dest>::<module>` into the same file.
- `stays_behind_through_a_body` (`preconditions.rs:85-120`) and `paths_naming_the_origin` (`cluster/stranded.rs:128ff`) read the member file only.
- `module_declaration` (`manifest_edits.rs:6-19`) and `declared_module_name` (`:61-65`) strip only `"pub "`. Callers: `preconditions.rs:163,197`, `facade_writer.rs:137,227`, `module_home.rs:196,239`.
- `facade_lines_for_plan` (`crate_move.rs:346-368`) writes `pub use` grouped per destination. `written_facade` (`facade_writer.rs:184-223`) matches `pub use {extern}::` only.
- `Scan::continues_a_path` (`source_scan.rs:137-143`) treats any preceding `.` as a field access, `..` included.
- Dry run and apply both report `edit.changes.len()` (`runner/entry_points/store_run.rs:320,223`). No crate move adds notes (`backends/rust.rs:1286,1329`).
- `relocation::plan` (`backends/rust/module_reparent/relocation.rs:32-54`) is the relocation rule, private to `backends/rust`.

### State B (Target)

A `foo.rs` + `foo/` module moves in one plan line, with every child under `<dest>/src/foo/` and its paths and crates handled. A restricted `mod` moves, and its facade keeps its visibility. No moved file names its own crate. `..crate::…` is re-pointed. `check` reports every child that cannot be carried. The dry run and apply print the same count, plus a note naming the moved files.

### Delta (What's Changing)

#### `tddy-code-restructuring`

- **`crate_move/module_files.rs`**: `MovedFile` and `relocated`, the relocation rule moved down from `relocation.rs`. The tested walker is reused unchanged.
- **`crate_move/moving.rs`**: `Move::carried` (`files_of(&self.source)` → `relocated(children_directory-parent, <dest>/src, module, …)`) and `Move::declaration`.
- **`crate_move/carried.rs`** (new, ~180 lines): `fold_carried_members`, `uncarriable` (the static findings), `restricted_children_reached`, `carried_notes`.
- **`crate_move/cluster.rs`**: `named_by` folds members. `resolve_cluster`'s loop body becomes `member_changes`, which:
  - iterates `member.carried()`: one rename and one `repointed_header_at` per file;
  - unions the headers' `crates_named`/`origin_paths` for `refuse_a_dependency_cycle`;
  - surveys each carried file's outside references;
  - calls `restricted_children_reached`.

  `travelling` includes every carried file.
- **`crate_move/header.rs`**: `repointed_header_at`, where the survey module path is `home.path ++ below`. `reach`'s `stays_inside_the_module` already keys on the tree root (`home.path`), so a `super::` inside the tree stays as written.
- **`crate_move/moving/facade_writer.rs`**: `left_behind` skips members whose `declared_in` is travelling. `leaving` builds `FacadeEntry`s with `Move::declaration().visibility`. `WrittenFacade.visibility` is added, and `written_facade` matches on it.
- **`crate_move.rs`**: `mod carried;`, `FacadeEntry`, `facade_lines_for_plan` grouping per (destination, visibility) and writing `"{vis} use …"` (`"use …"` when private), and `resolution`/`cluster_resolution`.
- **`crate_move/preconditions.rs`**: `move_preconditions` calls `carried::uncarriable`. `stays_behind_through_a_body` loops over carried files through a new helper `body_path_left_behind(text, module_path, …)`, so the function itself does not grow.
- **`crate_move/cluster/stranded.rs`**: `paths_naming_the_origin` reads each carried file's header.
- **`crate_move/manifest_edits.rs`**: `ModuleDeclaration`, `declared_module` (uses the visibility reader moved here from `facade_writer::after_visibility`), and `module_declaration`/`declared_module_name` delegating to it.
- **`crate_move/source_scan.rs`**: `continues_a_path` is false when the preceding `.` is itself preceded by `.` (`..`), or for `..=`.
- **`backends/rust/module_reparent/relocation.rs`**: `plan` → `module_files::relocated`.
- **`backends/rust.rs`**: `:1286` and `:1329` call `crate_move::resolution` / `cluster_resolution`. Line-neutral.

## Implementation milestones

- [ ] **M1** restricted declarations and the merge check; tests 21, 22, 19, 20
- [ ] **M2** `..` paths; tests 23, 24, 25
- [ ] **M3** `relocated` (reparent suites stay green) and `Move::carried`; renames, headers, manifest; tests 1–5
- [ ] **M4** travelling set, callers, `also` folding, no self facade; tests 6–11
- [ ] **M5** refusals: static uncarriable, carried bodies, restricted child; tests 12–18
- [ ] **M6** facade visibility; tests 26–28
- [ ] **M7** notes, count parity, live compiled cases; tests 29–31
- [ ] **M8** docs staged, scoped gate, function-size and file-length check, todos narrowed or deleted at wrap

## Testing plan

### Testing Strategy

**Primary: library level, no rust-analyzer.** The deciding half is driven by a fake `ModuleReferences` (the `AKnownReferenceSet` pattern from `cluster.rs:343-392`) over tempdir workspaces, through the public `crate_move::resolve` / `resolve_cluster` / `unrunnable_moves`. The edits and findings are exact text, checked in milliseconds.

**Thin live layer:** three cases in existing live binaries, with `assert_compiles_with_its_tests` as the oracle and the runner's dry-run/apply lines read through `a_sink_that_keeps_what_it_hears`.

#### Option 1 (chosen): new server-free binaries
- `tests/crate_move_children.rs`. Fixture: `origin` with `src/a.rs` (`pub mod b; mod c; mod i { mod d; }`, `crate::`/`super::` paths), `a/b.rs`, `a/c/mod.rs`, `a/c/e.rs`, `a/i/d.rs`, and `runtime.rs` (a caller). `origin/Cargo.toml` declares `shared` and the dev-dependency `proptest`; the destination is empty.
- `tests/crate_move_restricted_declarations.rs`.

**Trade-off:** exact and fast, but does not prove compilation.

#### Option 2 (chosen, thin): live cases in already-registered binaries
`tests/move_module_to_crate_acceptance.rs` and `tests/cluster_move_acceptance.rs`, both already in the filterset and the `rust-analyzer` group.

#### Option 3 (rejected): grow `cluster.rs`'s unit module
Rejected because the file is already 921 lines.

### Coverage Requirements

- [ ] Happy: `x.rs` and `x/mod.rs` children, inline-module children, a nested anchor, `also` children, `reexport` glob and none
- [ ] Paths: child `crate::` re-pointed, `super::` inside the tree kept, path out of the tree re-pointed, `..` paths
- [ ] Manifest: crates only children name; dev-only crates
- [ ] Refusals: each uncarriable shape (static), the restricted child (resolve-time), carried-body cycle
- [ ] Visibility: `pub(crate)`, `pub(super)`, `pub(in …)`, `pub(self)`, private; grouping; extension
- [ ] Pins: a lone nested member still flattens; `pub mod` facades unchanged; existing suites untouched
- [ ] Actual effects: bytes on disk; `cargo check --all-targets`

## Acceptance tests

Names read as behaviour specifications. Unless marked *green pin*, each test is **red on `master`**, for the reason given in its group heading.

### `packages/tddy-code-restructuring/tests/crate_move_children.rs` (new; library level, fake reference set)

These are red because `resolve_cluster` renames one file per member, flattens nested members, and writes facades into travelling files.

1. `a_moved_module_carries_every_file_its_mod_declarations_lead_to_in_the_same_edit`: renames for `a.rs`, `a/b.rs`, `a/c/mod.rs`, `a/c/e.rs`, `a/i/d.rs`.
2. `a_carried_tree_lands_at_the_same_relative_places_under_the_destination`: anchor `src/x/a.rs` → `dest/src/a.rs`, and `src/x/a/b.rs` → `dest/src/a/b.rs`.
3. `the_destination_root_declares_the_moved_module_and_none_of_its_children`
4. `a_carried_childs_crate_paths_are_re_pointed_and_its_super_paths_are_left_as_written`
5. `a_crate_only_a_child_names_joins_the_destination_dependencies_and_one_only_its_tests_name_joins_dev_dependencies`
6. `a_caller_of_an_item_in_a_carried_child_is_re_pointed_when_no_facade_is_asked_for`: `crate::a::b::Item` → `destination::a::b::Item`.
7. `a_reference_from_inside_the_carried_tree_is_not_a_caller`
8. `an_also_member_that_is_a_directory_child_of_another_member_is_carried_at_its_nested_position`
9. `the_moved_parents_mod_line_for_a_carried_child_is_left_as_written`
10. `no_file_arriving_in_the_destination_names_the_destination_crate`: glob and none; the parent's `pub use b::*;` is kept.
11. `a_nested_member_whose_parent_stays_behind_still_lands_at_the_destination_root`: *green pin*.
12. `a_restricted_child_reached_from_outside_the_tree_is_refused_naming_the_child_and_the_caller`: `pub(crate) mod b;`, with a caller writing `crate::a::b::Item`.
13. `a_carried_childs_path_back_into_the_origin_makes_the_move_refuse_as_a_cycle`: the cycle refusal lists the child's origin path.

### `packages/tddy-code-restructuring/tests/check_precondition_parity.rs` (existing; static `unrunnable_moves`, new tests appended)

These are red because no static pass opens a child file and `module_declaration` misses restricted lines.

14. `a_static_check_reports_a_carried_mod_line_that_leads_to_no_file`
15. `a_static_check_reports_a_carried_child_placed_with_a_path_attribute`
16. `a_static_check_reports_a_carried_file_whose_target_already_exists_in_the_destination`
17. `a_static_check_reports_a_carried_child_another_operation_of_the_plan_moves`
18. `a_static_check_reports_a_carried_childs_body_reaching_a_module_that_stays_behind`
19. `a_static_check_accepts_a_module_declared_pub_crate_pub_super_or_pub_in`: no "declares no `mod`" finding.
20. `a_destination_declaring_the_module_pub_crate_is_reported_as_a_merge`

### `packages/tddy-code-restructuring/src/crate_move/manifest_edits.rs` (unit module)

21. `finds_a_module_declared_with_any_visibility_with_its_span_and_visibility`: none, `pub`, `pub(crate)`, `pub(super)`, `pub(self)`, `pub(in crate::a)`; `modx;` and `// mod x;` are not declarations.
22. `places_a_new_declaration_in_sorted_position_among_restricted_ones`

### `packages/tddy-code-restructuring/src/crate_move/source_scan.rs` (unit module)

23. `reads_a_path_after_a_struct_update_or_a_range_operator_as_a_path`: covers `..crate::a::f()`, `0..crate::a::MAX` and `..=crate::a::MAX`, while `x.crate_field` is still not a path.

### `packages/tddy-code-restructuring/tests/crate_move_children.rs` (continued)

24. `a_struct_update_path_into_a_co_moving_module_is_re_pointed`: the R8 shape, `..crate::connection_service::starting_session_metadata(…)` forwarded to a co-moving `service_util`.

### `packages/tddy-code-restructuring/tests/check_precondition_parity.rs` (continued)

25. `a_struct_update_path_into_a_module_staying_behind_is_a_finding_naming_the_line`

### `packages/tddy-code-restructuring/tests/crate_move_restricted_declarations.rs` (new; library level)

These are red because the move is refused or every facade is `pub use`.

26. `a_pub_crate_module_moved_with_a_glob_facade_leaves_a_pub_crate_facade`, and `a_pub_super_module_in_a_nested_parent_leaves_a_pub_super_facade`, each a test function of its own.
27. `a_private_module_moved_with_a_glob_facade_leaves_a_private_use` (F2), and `two_pub_crate_modules_moved_to_one_destination_leave_one_grouped_pub_crate_line_beside_a_pub_line`.
28. `a_pub_module_moved_with_a_glob_facade_still_leaves_pub_use`: *green pin*.

### `packages/tddy-code-restructuring/src/crate_move/moving.rs` (unit module)

29. `extends_an_earlier_facade_of_the_same_visibility_and_leaves_one_of_another_visibility_alone`: red, because `written_facade` matches only `pub use`.

### Live: `packages/tddy-code-restructuring/tests/move_module_to_crate_acceptance.rs` (existing, registered)

30. `relocates_a_module_with_its_directory_children_and_every_crate_compiles_with_its_tests`. It also asserts that the dry run's and the apply's `-> N file(s)` lines are equal and that the carried note is printed. Red: `E0583` at the gate today.

### Live: `packages/tddy-code-restructuring/tests/cluster_move_acceptance.rs` (existing, registered)

31. `a_cluster_naming_a_child_in_also_nests_it_and_every_crate_compiles`. Red: the child is flattened and the parent gets a self facade (`E0432`).

### Existing suites, unchanged (*green pins*)

32. `tests/cluster_move.rs`, `tests/nested_module_move_acceptance.rs`, `tests/move_facades_acceptance.rs`, `tests/move_paths_acceptance.rs`, `tests/reparent_module_acceptance.rs`, `tests/reparent_module_beyond_the_basics_acceptance.rs` and the `cluster.rs`/`module_files.rs` unit modules all pass without edits.

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by the developer (2026-10-09, PRD review), with all recommendations accepted:

- **F1: carry children implicitly.** A module without its children does not compile. `also` children are folded, not refused.
- **F2: the facade mirrors the declaration's visibility exactly**, including private → `use dest::x;`. This stops `pub use` silently widening the origin's API.
- **F3: refuse a restricted child that something outside the moved tree reaches.** The refusal names the child and the callers. Widening is a follow-up (new todo).
- **F4: one relocation rule** in `crate_move/module_files.rs`; `reparent_module` delegates to it.

Binding stack decisions that shape this node: `libc` and target tables go to node 9; `pub(in crate::<origin>)` inside moved code goes to node 8; listed functions may not grow.

Taken by this plan:
- Count semantics are unchanged (the dry run already equals apply). Moved files are named in a note.
- `resolve`/`resolve_cluster` keep returning `WorkspaceEdit`. New `resolution`/`cluster_resolution` wrap them with notes, so no existing test changes.
- A nested member whose parent stays behind keeps today's root landing.

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

### Commit 2: contract surface and acceptance tests (2026-10-09)

**Baseline (scoped):** `cargo check -p tddy-code-restructuring --all-targets`, `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings` and `cargo fmt --all --check` were clean on the rebased commit 1 and are clean with the surface. Only the binaries and filters listed below were run; the full suite was not (see `scratchpad/reshape/baseline-failures.txt`: none).

**Red, each for the missing feature:**

| # | Test | Why it is red today |
|---|---|---|
| 1 | `tests/crate_move_children.rs:288` `a_moved_module_carries_every_file_its_mod_declarations_lead_to_in_the_same_edit` | Only `x/a.rs` is renamed |
| 2 | `:311` `a_carried_tree_lands_at_the_same_relative_places_under_the_destination` | `top/leaf.rs` is not renamed |
| 4 | `:352` `a_carried_childs_crate_paths_are_re_pointed_and_its_super_paths_are_left_as_written` | The child is not re-pointed (`crate::x::a::c` stays) |
| 5 | `:371` `a_crate_only_a_child_names_…_dev_dependencies` | The destination manifest gains neither `shared` nor `proptest` |
| 6 | `:395` `a_caller_of_an_item_in_a_carried_child_is_re_pointed_when_no_facade_is_asked_for` | The child's callers are not surveyed |
| 7 | `:414` `a_reference_from_inside_the_carried_tree_is_not_a_caller` | The child is not re-pointed |
| 8 | `:433` `an_also_member_that_is_a_directory_child_of_another_member_is_carried_at_its_nested_position` | `b` is flattened to `src/b.rs` and declared at the root |
| 9 | `:463` `the_moved_parents_mod_line_for_a_carried_child_is_left_as_written` | `pub mod b;` becomes `pub use destination::b;` |
| 10 | `:481` `no_file_arriving_in_the_destination_names_the_destination_crate` | The moved `a.rs` names `destination::` (the self facade) |
| 12 | `:531` `a_restricted_child_reached_from_outside_the_tree_is_refused_naming_the_child_and_the_caller` | Resolves `Ok` |
| 13 | `:559` `a_carried_childs_path_back_into_the_origin_makes_the_move_refuse_as_a_cycle` | Resolves `Ok` (the child's header is never read) |
| 24 | `:585` `a_struct_update_path_into_a_co_moving_module_is_re_pointed` | `..crate::x::a::defaults()` is left as written |
| + | `:607` `a_single_module_move_carries_the_same_files_as_a_cluster_of_one` (added: `move_module_to_crate` path) | 0 child renames |
| 14–18 | `tests/check_precondition_parity.rs:621, 642, 671, 702, 730` | No finding (the static pass opens no child) |
| 19 | `:755` `a_static_check_accepts_a_module_declared_pub_crate_pub_super_or_pub_in` | "declares no `mod workspace_session`" |
| 20 | `:800` `a_destination_declaring_the_module_pub_crate_is_reported_as_a_merge` | No merge finding |
| 25 | `:826` `a_struct_update_path_into_a_module_staying_behind_is_a_finding_naming_the_line` | No finding (`..crate` is not sighted) |
| 21 | `src/crate_move/manifest_edits.rs:569, 590` (`finds_a_module_declared_with_any_visibility…`, `a_comment_or_a_longer_name_is_not_a_declaration`) | `todo!("declared_module")` |
| 21 | `:602` `module_declaration_finds_a_restricted_declaration` | `None` |
| 22 | `:617` `places_a_new_declaration_in_sorted_position_among_restricted_ones` | Restricted neighbours are not read |
| — | `src/crate_move/module_files.rs:191` `relocates_each_file_of_a_module_to_the_same_place_under_the_new_directory` | `todo!("relocated")` |
| 23 | `src/crate_move/source_scan.rs:466` `reads_a_path_after_a_struct_update_or_a_range_operator_as_a_path` | `0..crate::…` and `..crate::…` are not sighted (`..=` already is: the preceding `=` breaks the continuation, so the plan's claim about `..=` was wrong) |
| 26 | `tests/crate_move_restricted_declarations.rs:104, 129` | The move is refused ("declares no `mod`") |
| 27 | `:154` `a_private_module_moved_with_a_glob_facade_leaves_a_private_use` | Writes `pub use destination::config;` |
| 27 | `:176` `two_pub_crate_modules_…_beside_a_pub_line` | Refused ("declares no `mod`") |
| 29 | `src/crate_move/moving.rs:404` `extends_an_earlier_facade_of_the_same_visibility_…` | `written_facade` ignores `visibility` |
| 30 | `tests/move_module_to_crate_acceptance.rs:85` `relocates_a_module_with_its_directory_children_and_every_crate_compiles_with_its_tests` (live) | The compile gate fails (child stranded); the dry run and apply both say `6 file(s)`, with no note |
| 31 | `tests/cluster_move_acceptance.rs:173` `a_cluster_naming_a_child_in_also_nests_it_and_every_crate_compiles` (live) | The compile gate fails (child flattened to `src/clock_face.rs`) |

**Green pins:**
- `tests/crate_move_children.rs:334` `the_destination_root_declares_the_moved_module_and_none_of_its_children` (planned red; today's move declares only `a` because it never sees the children, so this guards green from declaring them).
- `:506` `a_nested_member_whose_parent_stays_behind_still_lands_at_the_destination_root` (11).
- `tests/crate_move_restricted_declarations.rs:203` (28).
- `src/crate_move.rs:479` `modules_of_two_visibilities_moved_to_one_destination_leave_one_line_per_visibility` (the already-implemented `facade_lines_for_plan`).
- The existing suites in the touched binaries (`check_precondition_parity.rs`: 15 pass; `crate_move::facade_tests`, `moving::tests`, `module_files::tests`, `source_scan::tests`, `sorted_declaration_tests`: all pass).

**Not this node's:** `backends::rust::facade::facade_tests::a_named_facade_*` (4) fail under `--lib`. They are node 3's contract reds.

**Live registration:** none needed. Both live tests were added to binaries already in `.config/rust-e2e.filterset` and the `rust-analyzer` group. A new shared fixture is in the harness: `a_workspace_whose_module_has_a_directory_child`, `A_DIRECTORY_CHILD`.

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-move-children-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-move-children.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point, not edited while planning)
- [x] Create failing acceptance tests (commit 2: contract surface and tests)
- [x] Run acceptance tests (verify they fail) — see `## Validation Results`
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests (surface unit tests in commit 2)
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
- [ ] Wrap documentation (/wrap-context-docs) when the PR is set ready for review. This step also:
  - deletes `2026-10-09-reshape-move-children-initial-discovery.md`;
  - deletes the four fully resolved todos and the two closable 2026-09-09 records;
  - **narrows** `2026-10-04-…-directory-children-behind.md` (libc → node 9; doc comment → node 3) and `2026-10-08-…-also-members-…-and-their-crates.md` (create-crate, `async-trait`, `libc` → node 9) to whatever of those slices has not landed.
- [ ] USER REVIEW — work complete, decide next steps
