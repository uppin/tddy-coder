# Facades a cross-crate move leaves behind

What `move_module_to_crate` and `move_cluster_to_crate` write in the origin and in the destination's
root once the paths are right (the [path survey](path-survey.md) decides those). Behaviour is
described in the [feature doc](../../../docs/ft/coder/rust-code-restructuring.md); this page is where
the code is and what its limits are.

## Where the writing happens

| Concern | Code |
|---|---|
| One grouped line per destination | `crate_move.rs` `facade_lines_for_plan` — destinations in first-moved order, modules sorted; a single module is `pub use dest::module;` |
| What a set of moves leaves in a declaring file | `crate_move/moving.rs` `left_behind` → `leaving`, with `earlier_facade`, `extended_facade`, `declaration_of`, `parent_reexport_edits` |
| Sorted `pub mod` in the destination root | `crate_move/manifest_edits.rs` `insert_module_declaration_sorted`, called by `moving.rs` `declared_in_destination` |
| A test-binary move through a crate-root facade | `crate_move/module_home.rs` `crate_root_facade_forwarding`, consulted by `defining_module_in_crate` |

`Reexport::Named` and `Reexport::None` do not go through `facade_lines_for_plan`; `facade_line`
answers them, and answers nothing for `Reexport::Glob`.

## Extending a line an earlier operation wrote

`leaving` extends an existing `pub use <dest>::{…};` in the declaring file rather than adding a second.
It does so only when every module the line names is declared `pub mod` in the destination root, which
is how a move declares what it carries. **A hand-written `pub use <dest>::alpha;` for a module the
destination also declares `pub mod` cannot be told from the tool's own line and is extended too.** A
unit test pins that limit.

## Parent re-exports

`parent_reexports_of` reads lines: a `use <module>::*;` or `use <module>::{…};` at column 0 and brace
depth 0, whatever its visibility. Text after `//` is ignored when counting depth; a brace inside a
string literal can still throw the count off. A parent glob names no items, so nothing is counted as
reached from outside the crate — only the rewrite is done.

## Following a facade to its crate

`crate_root_facade_forwarding` handles `pub use <crate>::*;` only when `<crate>` is a dependency of the
origin by `path`. The dependency's root is its `[lib] path` (`manifest_edits::lib_path`) or
`src/lib.rs`. The origin's own root is still taken to be `src/lib.rs` in `defining_module_in_crate`.

## Verification

`tests/move_facades_acceptance.rs` runs against live rust-analyzer: one grouped facade with a clean
`cargo clippy -D warnings`, a destination name the origin also binds, sorted declaration, a nested
module's parent glob, a `mod tests` `crate::` path, the `super::*` guard and a test-binary move after a
module move. The unit tests sit beside the code they pin.
