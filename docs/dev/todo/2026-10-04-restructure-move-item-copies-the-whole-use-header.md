# 2026-10-04 — `move_item` copies the source module's whole `use` header into the destination

**Category:** Known limitation (engine quality)
**Source:** `move_item` (`item_move/imports.rs`), changeset
[`2026-10-04-restructure-same-crate-moves`](../1-WIP/2026-10-04-restructure-same-crate-moves.md), E1

## What the engine does

The moved items arrive in the destination without the `use` items that gave their names meaning. The engine
copies every top-level `use` of the source module across (attributes kept, a relative head written from the
crate root, anything the destination already binds left out), plus one `use` for each item the source keeps
that the moved code names.

## Why the whole header

A trait import has no name in the code that needs it (`use std::fmt::Write;` for `write!`), so no reading of
the moved text can tell which imports to leave out. `extract_module` asks the server which names are
unresolved and which import fixes each; that pass is written around a new *child* module of the parent and
does not carry over to a sibling in another file.

## What it costs

The destination gets imports it does not use. At the end of a **complete** run the unused-import tidy
(`runner/tidy.rs`) removes them on the compiler's own evidence, so the result is lint-clean (pinned by
`move_item_beyond_the_basics_acceptance`). A run stopped early (`--stop-after`) skips the tidy and leaves the
surplus as `unused_imports` warnings; the operation says so in its notes.

## What the engine should do

Ask the server which of the copied statements the moved code resolves through (semantic tokens over the
destination after the move, as `extract_module`'s `unresolved_names` does), and drop the rest at resolve time.
That makes a partial run clean and removes the dependency on the tidy.

**2026-10-05 update (moving the lifecycle items):** the copied header is now filtered for **reachability**:
an import of a module the destination cannot see is dropped when the moved code names nothing from it,
or the module's declaration is widened when it does; a group the destination binds part of is written
as one `use` per unbound name. The header is still copied whole otherwise (trait imports have no name
in the moved text), so the unused-import tidy remains what removes the surplus.
