# Restructure same-crate moves - PRD

**Date**: 2026-10-04
**PRD Type**: Feature Enhancement (new plan operations) + Technical Improvement (tool ergonomics)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md): two new Rust plan
  operations (`move_item`, `reparent_module`), their entries in § Rust operations (v1), § Item anchors
  and § Known limitations, and three ergonomics changes to `anchors`, `snapshot` and the warm daemon.
- **Related Feature**: [Warm code intelligence daemon](../warm-code-intelligence-daemon.md): a `warm`
  subcommand and `run-index-daemon` warming the checkout's root.
- **Related surface**: `.agents/skills/code-restructuring/SKILL.md` and its `references/plan-schema.md`:
  the authoring workflow an agent follows. Not a `docs/ft/` document, but the operative instruction set.

## Summary

`tddy-tools restructure` can move a whole module to another crate, but inside one crate it can only
group items into a module of the file they are already in. Splitting a crate by topic therefore stops at
every item that sits in the wrong file and every module that sits under the wrong parent: the author has
to `git mv` and edit `mod` and `use` lines by hand, and the engine's own guarantees (the compile gate,
`verify`, comments kept, imports re-pointed from the server's own reference set) do not cover the result.

This PRD adds the two missing operations, so a plan can say "move this item into that module" and "move
this module under that parent", and removes three frictions met while using the tool.

## Background

`#carve` 16a (PR #531) converted `tddy-session-lifecycle`'s leaf topics in place and had to defer four
steps because the engine could not do them: grouping four free functions from four files into one
module, splitting one file between two topics, re-parenting module files, and grouping two functions
with a topic module. Each was filed as a TODO with the missing operation named. The evidence is in
[the discovery companion](../../../dev/1-WIP/2026-10-04-restructure-same-crate-moves-initial-discovery.md).

Three smaller frictions came with the same run: a package-relative path to `anchors` is refused with a
message that does not say what to change; the first request after the index daemon starts silently waits
minutes for the crate graph; and `restructure snapshot` exited with `lsp server exited`.

## Proposed Changes

### `move_item`: move items into another module of the same crate

A plan operation that moves a run of items, in any file of a crate, into an **existing** module of that
crate (inline or in its own file).

- **Anchor**: an `items` anchor, as for `extract_module` (one file, a contiguous run of sibling items).
- **`to`**: the destination module's path, rooted at the package name like an item path
  (`my_crate::connection_service::peer_session_answer`). It must already exist: files are never created
  by a move. A module that does not exist yet is created first, by `extract_module` or by an earlier
  `reparent_module`.
- **`reexport`**: `glob`, `named` or `none`. With `glob`/`named` the source module keeps a `pub use`
  line and **no caller changes**. With `none` (or absent) every caller is re-pointed to the new path,
  from the language server's own reference set, which is what makes the module a real topic boundary.
- **What it keeps**: doc comments, attributes and ordinary comments on the moved items travel with them
  byte for byte; the items' own `use` needs are restored; visibility is widened only as far as a caller
  needs, and every widening is reported.
- **What it refuses, naming the cause**: a destination that does not exist; a name the destination
  already declares; a destination that is the item's own module; an item inside an `impl` block (an `impl`
  moves as an item of its own). A private item that moved code reaches is *widened*, not refused.

### `reparent_module`: move a module under a different parent

A plan operation that moves a module's file, and the directory of its children, so another module of
the same crate declares it.

- **Anchor**: an `items` anchor on the module's `mod` declaration in its old parent, the same kind `move_item` takes.
- **`to`**: the new parent's module path. It must already exist.
- **Effect**: the file (and directory) is moved with `git mv`; the old parent stops declaring the module
  (in the `<parent>.rs` or `<parent>/mod.rs` form) and the new parent declares it with the same
  visibility and attributes; every path to the module is re-pointed; `super::` paths inside the moved
  tree are rebased.
- **`reexport`**: `glob` leaves `pub use` in the old parent so callers keep resolving; `none` re-points them.
- **Refuses**: a destination that does not exist, a name the destination already declares, a destination
  inside the module being moved, and a module placed with `#[path]`.

### Ergonomics

1. **`anchors` with a path that is relative to a package** names the repo-root path to write instead of
   saying "no package". The path is **not** resolved silently.
2. **`restructure warm`** loads the tree's crate graph into the index daemon and reports progress until
   it is queryable; `run-index-daemon` runs it after the daemon answers (opt out with `--no-warm`), so
   the first real request no longer pays for the load.
3. **`restructure snapshot` of a plan with item anchors** goes to the warm index daemon when one is configured
   (a `Snapshot` RPC), instead of starting a rust-analyzer of its own. A plan with no item anchors still needs
   no server and stays in process.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring`: two new `RefactorKind`s with their own modules (the Rust backend only
  dispatches), plan-codec rules, harness builders and acceptance suites.
- `tddy-tools`, `tddy-index-daemon`, `run-index-daemon`: the `warm` subcommand and its wiring.
- `tddy-session-lifecycle`: the deferred `#carve` 16a items are applied with the new operations. No
  behaviour change, no public path change.

### User Impact

Authors of restructure plans get the missing moves, with the same safety net as the existing
operations. No existing plan changes meaning: both kinds are new, and `warm` is a new subcommand.

## Implementation Plan

See [the changeset](../../../dev/1-WIP/2026-10-04-restructure-same-crate-moves.md).

## Acceptance Criteria

- [ ] A plan with `move_item` moves items between files of one crate; the tree compiles with its tests,
      callers are re-pointed (or a facade is left), comments and attributes survive.
- [ ] A plan with `reparent_module` moves a module and its directory under another parent, in both
      parent file forms; the tree compiles.
- [ ] Each refusal above is reported before a server is spawned where the text answers it, naming the cause.
- [ ] `anchors` with a package-relative path names the repo-root path.
- [ ] `restructure warm` without a daemon refuses naming `./run-index-daemon`; with one, the daemon reports
      the root warm.
- [ ] `restructure snapshot` of an item-anchored plan is answered by the warm daemon when `TDDY_INDEX_SOCKET` is
      set, and never starts a rust-analyzer beside it.
- [ ] The `#carve` 16a deferred items are applied through the new operations: the lifecycle baseline
      holds by test name and the module/ownership checks hold.

## References

- [Rust code restructuring](../rust-code-restructuring.md)
- Backlog entries filed by `#carve` 16a (named in the changeset's Prerequisites)
