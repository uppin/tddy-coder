# 2026-10-04 — `reparent_module` widens the module's declaration, not the items inside and around it

**Category:** Known limitation (engine capability)
**Source:** `reparent_module` (`packages/tddy-code-restructuring/src/backends/rust/module_reparent/`), changeset
`2026-10-04-restructure-same-crate-moves` (wrapped into `packages/tddy-code-restructuring/docs/same-crate-moves.md`), E2

## What the engine does

The `mod` declaration travels with its attributes and is respelled only as far as its callers need (the
`Scope` reading `move_item` uses): a private `mod attachments;` whose only caller is the old parent becomes
`pub(crate) mod attachments;` in the new one, reported as a visibility change. Relative paths in the moved
files (`super::host_name`, `use super::*;` in an inline test module) are rebased so they still reach the same
item; a `super::` that stays inside the moved tree is left alone.

## What it does not widen

Privacy that belongs to the module's contents or to the modules it used to sit under:

- **A private item of the old parent, or of an ancestor, that the moved tree names** (`super::host_name()`
  with `fn host_name` private in `host`): a child may reach its parent's private items, a module under a
  different parent may not. Reproduced 2026-10-04: the move applies, then the compile gate stops with
  `E0603: function `host_name` is private`. `move_item` widens the equivalent items it moves away from
  (`reached_by_the_moved_code`); a module move has no such survey.
- **A `pub(super)` / `pub(in path)` item of the moved module that its old parent names** (`pub(super) fn
  materialize` called as `attachments::materialize()` from `host`): `pub(super)` now means the new parent,
  so the old parent no longer sees it. Reproduced: `E0603` at the old parent's call.
- **A visibility written as an absolute path into the moved tree** (`pub(in crate::host::attachments)`) is
  not respelled, and names a module that is no longer there.

## What happens instead

The edit is applied, the compile gate runs, and the run stops with `AppliedTreeDoesNotCompile` naming the
`E0603` site: loud, but the whole index is paid for and the edit is left on disk to roll back.

## What the engine should do

Ask `textDocument/references` for each item of the moved tree that is not `pub`/`pub(crate)` (and each
private item of the old ancestors the tree names) and widen it to the narrowest scope the new arrangement
needs, with the same report line `move_item` writes; respell an absolute `pub(in …)` that points into the
tree. Until then the rebase leaves every visibility keyword in the moved files as written
(`rebase::Modules::travelling`).
