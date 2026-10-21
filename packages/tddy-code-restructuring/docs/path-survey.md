# Path survey

How a cross-crate move learns what the moved file names. The product contract is
[Rust code restructuring](../../../docs/ft/coder/rust-code-restructuring.md#path-survey).

## One reading, five consumers

`crate_move::survey::survey_moved_file` reads a moved file once, before any edit, and returns a
`PathSurvey`. Five things are derived from it and from nothing else, so they cannot disagree about
what the file names:

| Consumer | Reads | Writes |
|---|---|---|
| `header::repointed_header_at` | every surveyed path | the edits that re-point each path, headers and bodies; the crates the file names; the edges back to the origin |
| `refusals::refuse_a_dependency_cycle` | the edges | the cycle refusal, listing every path that forced it |
| `moving::Move::destination_manifest` | the crates named, split by `#[cfg(test)]` | `[dependencies]`, `[dev-dependencies]` and target-table lines, copied from the origin's manifest |
| `item_move::canonical_paths` (through `crate_move::survey`, now `pub(crate)`) | each `crate::`-headed path whose `defined_at` differs from what is written | the rewritten moved text, and a note naming every path rewritten or left |
| `repoint_facade::rewrite` (through `crate_move::survey`, `pub(crate)`) | each path whose `defined_at` differs from `resolved` **and** whose defining crate is not the file's own | the rewritten paths of one file (or of every file of one module), and a note naming every path rewritten |

`move_cluster_to_crate` surveys each member and passes the set of co-moving members, so a path reaching
a sibling that travels with it is `crate::`, not an edge. `check` reads the survey too, through
`Header::origin_paths`, the paths of every carried file that would become an edge.

## Carried files

A moved module is a set of files: its own and every file its `mod` declarations lead to
(`module_files::files_of`, inline modules followed). `Move::carried` returns that set, the module's
own file first, with each file's destination and its module path **below** the moved module
(`MovedFile.below`). Every carried file is surveyed by `repointed_header_at`, whose survey module path
is the module's path followed by `below`, and then feeds the same consumers as the module's own file:
its crates reach the destination manifest, its edges reach the cycle refusal, and its outside references
are surveyed for callers. A file's `self::` or `super::` path that stays inside the carried tree is left
as written, because the tree's root, not the file, is what `stays_inside_the_module` keys on.

The carried files join the travelling set, so a reference from inside the tree is not a caller. A
member of a cluster whose module path lies under another member's is folded into that member
(`carried::fold_carried_members`, applied in `read_members`), so it is carried at its nested position
rather than renamed to the destination's root. A nested member whose parent does **not** move still
lands at the destination root under its last segment (`header.rs`).

`module_files::relocated` is the one rule for where a carried file lands relative to the module: the
cross-crate move and `reparent_module` (`backends/rust/module_reparent/relocation.rs`) both call it.

## Test modules

A `#[cfg(test)] mod t;` declared in the file that declares a moved module, when that file stays, is a
sibling of the moved code. `source_scan::test_declarations` reads the top-level declarations with the span
of their attributes and doc comments and whether `#[path]` places one; `crate_move::test_modules` decides
which follow. Each considered module's files (`module_files::files_of`) are surveyed at the declaring
file's module path plus `t`. A path is read as naming the origin when `defining_crate` is the origin and
the path does not resolve inside the test module itself. It *reaches the move* when `resolved` or
`defined_at` is inside the moving set: members, their carried files, and the modules earlier operations
of the plan moved (`moved_by_earlier_operations` for a static check; in `apply` those moves are already in
the text the workspace overlay reads, so the resolve path passes none). The module follows when every
origin-naming path reaches the move and one does, and it is not placed by `#[path]`. At resolve time
`ModuleReferences::outside_references` keeps back a follower whose items a file outside the travelling set
names. Notes say which follow and which stay.

A follower is renamed with `module_files::relocated` beside the member it names first, its header goes
through `repointed_header_in` with `Gate::Test` and its own module as the tree root, its declaration leaves
the origin and is appended to the destination root, and its files join the travelling set before the caller
survey. `header::Gate::Test` marks every surveyed path `in_test`; `test_modules::test_gated` gives the same
reading to a carried file at or below a `#[cfg(test)]` declaration, looked up in the scope the inline
modules on the way down open. A static `unfollowable` check reports a landing that exists or is already
declared, with the text `apply` refuses with.

The header pass reads a path's `defined_at` as well as `resolved` for co-moving, and keys "stays inside
the module" on the file's own tree root (`header::FileInTree.tree_root`): a sibling's `super::moved` is
rewritten, a test module's `self::helpers` is not.

## The surface

`PathSurvey { paths: Vec<SurveyedPath> }`, `pub(crate)`, in the order the file writes them.

| `SurveyedPath` field | Meaning |
|---|---|
| `written` | the path as written: `super::helper`, `shared::Clock` |
| `resolved` | `self`/`super`/`crate` resolved against the file's module path, crate-rooted by extern name: `origin::outer::helper` |
| `defining_crate` | the extern name of the crate that defines the item, after re-exports are followed |
| `defined_at` | the crate-rooted path where the item is defined, after re-exports are followed: `destination::helper_mod::helper`. Equal to `resolved` when the path is not the origin's to forward. The rewrite needs the whole followed path, which `defining_crate` cannot carry; `check-parity` ([#543](https://github.com/uppin/tddy-coder/pull/543)) reads it for the same reason |
| `in_test` | written under a `#[cfg(test)]` item: the crate goes to `[dev-dependencies]` and the path is no edge |
| `in_body` | written in code rather than in a `use` item |
| `site` | where it is written, one-based; every leaf of one `use` tree shares the tree's |

`resolved_against(written, crate_name, module_path)` is the pure resolution step, exposed for the
other consumers.

## What is surveyed

A path is surveyed when its first segment is `crate`, `self` or `super`, or names a crate. In a `use`
item the first segment is a crate unless the file binds the name itself — a module it declares, a name
its `use` items bring in, an item it defines (`names_bound_by`) — and unless it is a built-in root
(`std`, `core`, `alloc`). In code the first segment must also be declared by the origin's manifest, in
any dependency table — plain, dev or target-specific (`declares_dependency_in_any_table`):
`PermissionMode::Plan` and `mpsc::channel` are paths too, and a dependency line for either would be an
edit nobody asked for. A `use` group with nothing before it names no path.

A crate imported under its own name is surveyed as that crate. `use async_trait::async_trait;` and
`use anyhow::{anyhow, Result};` bind a name that is also the head of their own path, which would read as
the file binding the head itself (`bound_only_by_their_own_crate`). A name is dropped from the bound set
only when every `use` binding it is a leaf of at least two segments, with no alias, whose first segment is
that name, nothing else binds it (no `mod x`, no item `x`, no `use other::x`), and the origin's manifest
declares it in some table. A module, an item or another import of the same name still shadows the crate.

## Resolution

1. **`self::`, `super::` (any depth) and `crate::` by segment.** The path is rooted at the origin's
   extern name and the file's module path (extended by the inline modules the path sits in), and each
   `super` pops one module. A `super` at the crate root is `MalformedPlan`. A path not starting with a
   relative qualifier is returned as written.
2. **Re-export following** (`reexports::followed`). A path that begins with the origin's extern name is
   walked module by module through the origin's sources, in the order Rust resolves a name: a child
   module, an item the module defines, an explicit `use`, then the module's globs, each of which must
   *confirm* the name before it counts. The walk is cycle-safe (a visited set) and follows chains of
   globs and of explicit re-exports. A glob from a crate the workspace does not reach by a path
   dependency cannot confirm a name and is not followed; an explicit `use` of one is, because the source
   says so. A path the walk cannot see further into comes back as written.
3. **Real read errors refuse.** The walk reads a module's text through the workspace overlay. Only
   `NotFound` means "no such module file"; any other I/O error is an error naming the file, so a
   permission failure never reads as a name that is absent.

`defining_crate` is the first segment of `defined_at`.

## Rewrite, edges, manifest

The rewrite (`header.rs`, `reach`) decides by where a path ends up: a path to a co-moving member stays
`crate::` (landing under the member's last segment; a `self::`/`super::` that stays inside the moved
module is left as written); a path whose defining crate is the destination becomes `crate::` plus the
rest of `defined_at`; anything else is written as its `defined_at`, and is an **edge back** when the
defining crate is the origin. A `use` tree is replaced by its prefix, which every member must agree on;
members that would need different qualifiers are rewritten as a whole statement by `crate_move::use_group`
(`split_or_reprefix`, Rule S: the kept members stay in the group, each lifted member follows as a `use`
of its own; a nested member whose leaves disagree is flattened; an attribute or doc comment above a
statement that must split is a `MalformedPlan` naming file and line), and a path spelled across
whitespace or comments is refused as one the rewrite cannot address. A plain `use` whose last segment
changes is rewritten `as` the old name.

`reach` also follows a path through a glob facade of the origin to the member it names, so a facade to a
co-mover is co-moving and one to a module that stays is an edge. A `pub(in crate::<module>)` restriction
is carried as `SurveyedPath.restriction` (read by `Sighting.in_visibility`): never an edge, never a
manifest line, respelled `pub(crate)` when the module stays and `pub(in crate::<landing>)` when it moves.
A cluster's callers are collected across its members (`SetTotals.callers`) and written together by
`moving::caller_changes`, one edit per statement.

Edges are collected outside `#[cfg(test)]` only. The manifest pass puts a crate in `[dev-dependencies]`
when only `#[cfg(test)]` code names it (and drops it when the rest of the file names it too).

Target-specific tables are read the same way (`manifest_edits::target_declarations`, headers matched
verbatim, quotes included: `[target.'cfg(unix)'.dependencies]`). `Move::dependency_lines` decides per named
crate: a crate the origin declares in the plain table being filled goes to that table; otherwise each
target table of the matching kind that declares it contributes its line, re-anchored, under the same
header in the destination — so `libc` stays `cfg(unix)`. Code that only `#[cfg(test)]` names lands in the
target `dev-dependencies` table, falling back to the target `dependencies` table the origin declares it in.
A destination that already declares the crate under that header, or in its plain table, gains nothing, and
a missing header is appended at the end of the manifest (`with_lines_under`). A crate in no table at all
refuses the move. The
destination appearing in either set is `MalformedPlan` from `destination_manifest` — an assertion, since
the survey reads the destination's own items as `crate::`.

## Widening

A cross-crate move widens in the same pass that surveys it. `ModuleReferences::outside_references`
returns **every declaration** of a moving file as `ItemReferences { item, referenced_at, declared_at,
within, kind }`: the name's position (one-based, in characters), the containers it sits in outermost first
(an inline module's name, `impl T`, `impl Tr for T`, a struct or enum name) and a `DeclarationKind` — `Item`,
`Field`, `InherentMember`, or `NoVisibility` for trait-impl members, trait items and enum variants.
`backends/rust/declarations.rs` reads them from the `documentSymbol` tree (identifier names only), so a
tuple field `0` is skipped. `surveyed` keeps its caller logic by reading only top-level `Item`s and carries
the whole list on `Survey.declarations`, plus the parent's re-export on `Survey.reexported_by`.

`crate_move/widening.rs` decides and writes, after `cluster_edits`, over the edit and the surveys:
`reach` (a declaration with a reference outside the moving files and outside the destination's directory,
plus the inline `mod`s on an item's path), `glob` (every non-private top-level item when the parent
re-exports by glob, found with `parent_reexports_of`), `escaping` (the fixpoint over types a widened
signature names, read from the masked text) and `declaration` (the visibility keyword at `declared_at`,
replaced with `pub`; a keyword on a line above its name is a `SeamRefused`). Edits merge into each member's
existing change in pre-move coordinates, one change per file. `resolved_cluster` returns the edit and the
`Vec<VisibilityChange>` report for both created and existing destinations; `cluster_resolution` puts the
report into `Resolution.report`, which the runner prints as `visibility:` lines and `check --deep`
prints through `rehearsed_lines`. `VisibilityChange.reason` is `None` for a plain reference and states
the glob or the signature otherwise; journal records written before it existed read it as `None`.

The cost is one `textDocument/references` request per declaration instead of per top-level item: on the
roster fixture of `move_to_crate_widening_acceptance`, nine instead of three. A request for a trait-impl
member is asked and its answer is not read.

Not widened: tuple-struct fields, `macro_rules!` items and in-file `use` re-exports, associated types of an
inherent `impl`, and an item only a macro expansion in the origin names. A shadowing type of the same
name that a widened signature seems to name is widened too.

## `source_scan.rs`

The token reading under the survey. The file is masked first — comments and literals blanked byte for
byte, so offsets still address the original text — and tokenised; what is read out is shapes only:
`a::b::c` sequences (`sightings`), `use` trees with groups expanded (`UseLeaf`), `mod` blocks, and the
item attribute that marks `cfg(test)`. An identifier behind `..` or `..=` opens a path (a struct update
`..crate::a::f()`, a range `0..crate::a::MAX`); an identifier behind a single `.` is a field access and
does not (`Scan::continues_a_path`). `items_of_module` returns a module's top level: its `mod`s, its
`use`s, the names it defines. Nothing in it resolves a name.

`test_binary.rs` keeps its own line-based scanner (`readable_spans`, `names_bound_in`,
`is_a_built_in_root`, `segment_length`); `source_scan` borrows four of them rather than copying them, so
the two scanners coexist. Consolidating them is the extraction `docs/code-issues/oversized-file-test-binary.md`
records.

## Limits

- **`#[cfg(test)]` is recognised as `#[cfg(test)]` and `#[cfg(all(test, …))]` only.** `any(test, …)`,
  `not(…)` and `cfg_attr` read as ordinary code, which errs the safe way: a crate lands in
  `[dependencies]` rather than missing from the build.
- **A target header is matched as written.** `[target.'cfg(unix)'.dependencies]` and
  `[target."cfg(unix)".dependencies]` are two headers, and only the one the origin spells is carried.
  `move_test_binary_to_crate` reads no target table of its own (`test_binary.rs` has its own scanner).
- **`#[path = "…"]` modules are not followed, and items a macro generates, or paths inside a macro's
  arguments, are not seen.** The compile gate catches what
  results.
- **Only the origin's re-exports are followed.** A path through another crate's re-export is left at
  that crate.
- **`check`'s stranded-sibling finding reads `Header::origin_paths` of every carried file**
  (`cluster/stranded.rs` `paths_naming_the_origin`), the same edges `apply`'s cycle refusal reads, so a
  nested `use` is read too; `remedy_for` names a crate-root item with "cut the dependency" and a module
  with the cluster remedy. The body-path finding (`stays_behind_through_a_body`) reads the survey's body
  paths of every carried file directly.
- **A restricted `use` that must both narrow its restriction and split is only narrowed** (`header/rewrite.rs`
  `rewrite_of` returns the narrowing edit first). No input has met it.
- **`survey.rs` is `pub(crate)`.** Its consumers are inside this crate.

## Testing

`tests/move_paths_acceptance.rs` runs against a real rust-analyzer over three-crate fixtures and
asserts the written text and that the tree compiles: a path through the origin's facade to the
destination, the destination named by its extern name in headers and bodies, the destination never in
its own manifest, a crate named only in a body (and only under `cfg(test)`), a `super::` import through
a glob re-export of the destination, a module import whose items the destination defines, a body path
to an origin item refusing the move, a `#[cfg(test)]` module naming an origin item, a mixed-qualifier
`use` group, and a `use Kind::*;` over an enum the moved file defines. Unit tests sit in `survey.rs`
(`super`/`self`/`crate` resolution, a path above the root, extern paths), `source_scan.rs` (masking,
`cfg(test)` scope, `use` expansion) and `reexports.rs` (cycle safety, `NotFound` against a real read
error, `cfg(all(test, …))`).
