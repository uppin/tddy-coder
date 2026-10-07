# Path survey

How a cross-crate move learns what the moved file names. The product contract is
[Rust code restructuring](../../../docs/ft/coder/rust-code-restructuring.md#path-survey).

## One reading, five consumers

`crate_move::survey::survey_moved_file` reads a moved file once, before any edit, and returns a
`PathSurvey`. Five things are derived from it and from nothing else, so they cannot disagree about
what the file names:

| Consumer | Reads | Writes |
|---|---|---|
| `header::repointed_header` | every surveyed path | the edits that re-point each path, headers and bodies; the crates the file names; the edges back to the origin |
| `refusals::refuse_a_dependency_cycle` | the edges | the cycle refusal, listing every path that forced it |
| `moving::Move::destination_manifest` | the crates named, split by `#[cfg(test)]` | `[dependencies]` and `[dev-dependencies]` lines, copied from the origin's manifest |
| `item_move::canonical_paths` (through `crate_move::survey`, now `pub(crate)`) | each `crate::`-headed path whose `defined_at` differs from what is written | the rewritten moved text, and a note naming every path rewritten or left |
| `repoint_facade::rewrite` (through `crate_move::survey`, `pub(crate)`) | each path whose `defined_at` differs from `resolved` **and** whose defining crate is not the file's own | the rewritten paths of one file (or of every file of one module), and a note naming every path rewritten |

`move_cluster_to_crate` surveys each member and passes the set of co-moving members, so a path reaching
a sibling that travels with it is `crate::`, not an edge. `check` reads the survey too, through
`Header::header_origin_paths`, but only the file's top-level `use` header (see Limits).

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
either dependency table: `PermissionMode::Plan` and `mpsc::channel` are paths too, and a dependency
line for either would be an edit nobody asked for. A `use` group with nothing before it names no path.

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
members that would need different qualifiers are `MalformedPlan` ("write one `use` per path"), and a path
spelled across whitespace or comments is refused as one the rewrite cannot address. A plain `use` whose
last segment changes is rewritten `as` the old name.

Edges are collected outside `#[cfg(test)]` only. The manifest pass puts a crate in `[dev-dependencies]`
when only `#[cfg(test)]` code names it (and drops it when the rest of the file names it too). The
destination appearing in either set is `MalformedPlan` from `destination_manifest` — an assertion, since
the survey reads the destination's own items as `crate::`.

## `source_scan.rs`

The token reading under the survey. The file is masked first — comments and literals blanked byte for
byte, so offsets still address the original text — and tokenised; what is read out is shapes only:
`a::b::c` sequences (`sightings`), `use` trees with groups expanded (`UseLeaf`), `mod` blocks, and the
item attribute that marks `cfg(test)`. `items_of_module` returns a module's top level: its `mod`s, its
`use`s, the names it defines. Nothing in it resolves a name.

`test_binary.rs` keeps its own line-based scanner (`readable_spans`, `names_bound_in`,
`is_a_built_in_root`, `segment_length`); `source_scan` borrows four of them rather than copying them, so
the two scanners coexist. Consolidating them is the extraction `docs/code-issues/oversized-file-test-binary.md`
records.

## Limits

- **`#[cfg(test)]` is recognised as `#[cfg(test)]` and `#[cfg(all(test, …))]` only.** `any(test, …)`,
  `not(…)` and `cfg_attr` read as ordinary code, which errs the safe way: a crate lands in
  `[dependencies]` rather than missing from the build.
- **`pub(in …)` visibility is not interpreted, `#[path = "…"]` modules are not followed, and items a
  macro generates, or paths inside a macro's arguments, are not seen.** The compile gate catches what
  results.
- **Only the origin's re-exports are followed.** A path through another crate's re-export is left at
  that crate.
- **`check`'s stranded-sibling finding reads the top-level `use` header only.**
  `Header::header_origin_paths` is the header subset of the edges, kept for that finding. The
  body-path finding (`stays_behind_through_a_body`) reads the survey's body paths directly; a `use`
  nested in a function is `in_use`, not `in_body`, so neither reads it.
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
