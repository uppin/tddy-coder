# 2026-10-09 — `reparent_module`'s clash check counts an import of the moving module as a declaration

**Category:** Restructure engine defect (found reading the code; no run has hit it)
**Source:** #reshape 11/19 (`feature/reshape/move-item-paths`), discovery E2.4

## What the engine does

`reparent_module` refuses a new parent that already binds the module's name
(`packages/tddy-code-restructuring/src/backends/rust/module_reparent/survey.rs`, `name_taken`) by reading
`names_declared_in` over the new parent's text. That set holds every name a `use` binds, so a new parent that
imports the very module being moved (`use crate::host::worker;` in `split.rs`, then `worker` reparented under
`split`) is refused as `E0428`, although the import is not a second declaration and the move's re-pointing
would remove it.

`move_item` already exempts this case (`item_move/preflight.rs`, `taken_by_something_else`: a destination
binding that resolves to the moving item, directly or through a glob re-export of its module).

## What the engine should do

Use the same exemption for `reparent_module`: a binding of the name that resolves (with
`item_move::use_path::resolved`) to the module being moved is not a clash, and the re-pointing drops it.

## Why deferred

No run has met it, and `reparent_module`'s first-cut limits belong to `feature/reshape/widen-same-crate`'s
claimed entry; #reshape 11/19 kept to the paths it was planned for (developer decision F5, 2026-10-09).
