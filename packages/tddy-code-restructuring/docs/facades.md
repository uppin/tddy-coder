# Facades a cross-crate move leaves behind, and the one an extraction leaves

What `move_module_to_crate` and `move_cluster_to_crate` write in the origin and in the destination's
root once the paths are right (the [path survey](path-survey.md) decides those). Behaviour is
described in the [feature doc](../../../docs/ft/coder/rust-code-restructuring.md); this page is where
the code is and what its limits are. The last section covers the facade an `extract_module` writes.

## Where the writing happens

| Concern | Code |
|---|---|
| One grouped line per destination and visibility | `crate_move.rs` `facade_lines_for_plan(&[FacadeEntry])` — (destination, visibility) pairs in first-moved order, modules sorted; a single `pub` module is `pub use dest::module;` |
| The visibility a facade keeps | `crate_move/moving.rs` `Move::declaration`, which reads the origin's `mod` line through `manifest_edits::declared_module` (span and visibility); `facade_writer.rs` `glob_entries` builds the `FacadeEntry`s |
| What a set of moves leaves in a declaring file | `crate_move/moving/facade_writer.rs` `left_behind` → `leaving`, with `earlier_facade`, `extended_facade`, `declaration_of`, `removals_with_separators`, `left_in_place_of`, `parent_reexport_edits` |
| The span a removed `mod` declaration covers | `crate_move/manifest_edits.rs` `removed_declaration`, `separates_use_runs`, `separator_carriers`; read by `leaving` and by `preconditions.rs` `move_preconditions` |
| Sorted `pub mod` in the destination root, with the declaration's comments | `crate_move/manifest_edits.rs` `insert_module_declaration_sorted`, called by `facade_writer.rs` `declared_in_destination` (comments read by `comments_of_declaration`) |
| A test-binary move through a crate-root facade | `crate_move/module_home.rs` `crate_root_facade_forwarding`, consulted by `defining_module_in_crate` |

`Reexport::Named` and `Reexport::None` do not go through `facade_lines_for_plan`; `facade_line`
answers them, and answers nothing for `Reexport::Glob`.

## The facade keeps the declaration's visibility

The glob facade is written with the visibility of the `mod` line it replaces, exactly: `pub mod x;` →
`pub use dest::x;`, `pub(crate) mod x;` → `pub(crate) use dest::x;`, `pub(super)`, `pub(self)` and
`pub(in …)` the same, and a private `mod x;` → a private `use dest::x;`. A `pub use` over a private module
would widen the origin's API. `facade_lines_for_plan` groups per (destination, visibility), so two
`pub(crate)` modules moved to one destination beside a `pub` one leave
`pub use dest::a;` and `pub(crate) use dest::{b, c};`. `manifest_edits::declared_module` reads a `mod`
line of any visibility (`module_declaration` and `declared_module_name` delegate to it), so a restricted
declaration is moved, found by the merge check and sorted in the destination like any other. It matches
a line of its own: an attribute on the same line (`#[cfg(unix)] mod x;`) is not read as a declaration.

## No facade into a file that moves

A member whose module path lies under another member's (a directory child named in `also`) is folded
into that member before the facades are written (`carried::fold_carried_members`). Its parent's
`mod child;` line and any `pub use child::…` stay as written in the moved parent, and the destination
root declares only the modules the plan moved. No file arriving in the destination is given a `use` that
names the destination crate.

A sibling `#[cfg(test)] mod t;` that follows the move (`crate_move::test_modules`) gets no facade either:
nothing outside it names it. Its declaration leaves the origin file with the attributes and doc comments
above it, and the destination root receives `#[cfg(test)]` + `mod t;` after its last line, not in sorted
position, because this repository keeps `#[cfg(test)] mod` lines after the production declarations.

## The declaration a move removes

A `mod <module>;` declaration is removed whole (`removed_declaration`). The span runs from the first of
the `///` and `//` lines directly above it (no blank line between) to the end of its line. Those lines
travel: `declared_in_destination` reads them from the declaring file and writes them above the
destination's `pub mod <module>;`.

Any `#[…]` or `#![…]` line directly above the declaration refuses the operation, naming the attribute
(`#[cfg]`, `#[path]`, `#[allow]` and `#[doc = …]` alike): a move to another crate can neither keep nor
drop one without changing what compiles. `move_preconditions` calls the same function, so plain
`check` reports the refusal and nothing is written. `module_declaration` decides which declarations
match; `removed_declaration` only widens the span upward from a match.

**Two `use` runs stay two.** When the nearest non-blank line above a removed span ends a `use` item and
the first line below it starts one (or the comment lines directly above one), removing the span would
join the runs, and `rustfmt` would sort the joined run. `separates_use_runs` detects it and `leaving`
leaves one blank line in the declaration's place: after the facade when a facade takes the place, alone
otherwise. A facade written there joins the run above it and may be moved by `rustfmt` within that run;
the run below stays apart. Declarations whose spans abut (adjacent lines, comments included) are weighed
as one by `separator_carriers`, and only the last of the group carries the blank line, so two adjacent
declarations between two runs leave exactly one.

`module_reparent` and `item_move` removals do not apply the use-run rule.

## Extending a line an earlier operation wrote

`leaving` extends an existing `<visibility> use <dest>::{…};` in the declaring file rather than adding a
second, for each visibility that has one; a line of another visibility is left alone and a facade of
that visibility is added beside it. It does so only when every module the line names is declared `pub mod`
in the destination root, which is how a move declares what it carries. **A hand-written
`<visibility> use <dest>::alpha;` for a module the destination also declares `pub mod` cannot be told
from the tool's own line and is extended too, whatever wrote it; a line naming anything the destination
does not declare is left alone.** Unit tests pin both.

## Parent re-exports

`parent_reexports_of` reads lines: a `use <module>::*;` or `use <module>::{…};` at column 0 and brace
depth 0, whatever its visibility. Text after `//` is ignored when counting depth; a brace inside a
string literal can still throw the count off. A parent glob names no items, so it counts nothing as
reached by itself; a cross-crate move treats every non-private top-level item of the module as reached
through it and widens those to `pub` (see [path-survey.md](path-survey.md#widening)), and `check --deep`
names the glob on the survey line.

## Following a facade to its crate

`crate_root_facade_forwarding` handles `pub use <crate>::*;` only when `<crate>` is a dependency of the
origin by `path`. The dependency's root is its `[lib] path` (`manifest_edits::lib_path`) or
`src/lib.rs`. The origin's own root is still taken to be `src/lib.rs` in `defining_module_in_crate`.

## The facade an `extract_module` leaves

`backends/rust/facade.rs` writes the facade an `extract_module` seam leaves in the parent; the survey
it reads (`backends/rust/seam_survey.rs`) says what moved and who reaches it.

### The survey waits and refuses

`survey_moved_items` and `survey_impl_members` read `documentSymbol` through `settled_outline`, the same
reader every other operation uses, so an empty outline is believed only once the server has been seen to
load. `refuse_unsurveyed_range` refuses a range whose original lines declare a named item or a child
module (`items_of_module`) while the survey found none, naming the range and the first item. A range of
`impl` blocks declares no named item and is not refused. `survey_moved_items` calls it only when the
outline lists no member of the range at all (`items_relocated_within` is empty): a range that cuts into
an `impl` is answered by an outline of its members, which no module path names, and an empty `moved`
there is correct.

### Named facade

`named_facade_lines` writes, in this order:

```
pub use m::{a, b};                 # pub items, carried whether or not reached
pub(crate) use m::{c};             # reached from production, by visibility tier, widest first
use m::{d};
#[cfg(test)]
pub(crate) use m::{t};             # reached only from the file's own #[cfg(test)] module
```

`is_carried` decides membership: a top-level item written `pub` is always carried, because its public
path is interface; a narrower item only when something outside the new module reaches it. A `pub` item
reached only from tests stays on the `pub` line. A carried item that is not `pub` and is reached only
from the file's own `#[cfg(test)]` module goes under a `#[cfg(test)]` line after the production lines.
`facade_will_bind` mirrors `is_carried`. An empty result keeps `empty_facade_note`; the nesting refusal
and the glob's width rule (`widest_visibility`: `pub` when anything moved is `pub`) are unchanged.

`reach_of` marks a reference `from_production` unless it lies inside an inline `#[cfg(test)] mod … { }`
of the same file (`seam_survey::test_module_lines`); a reference in another file always does.

## Verification

`tests/crate_move_restricted_declarations.rs` (library level) pins the facade visibilities and their
grouping, and the live `pub(crate)` module move in `tests/move_module_to_crate_acceptance.rs` compiles.

`tests/move_facades_acceptance.rs` runs against live rust-analyzer: one grouped facade with a clean
`cargo clippy -D warnings`, a destination name the origin also binds, sorted declaration, a nested
module's parent glob, a `mod tests` `crate::` path, the `super::*` guard, a test-binary move after a
module move, a doc comment that moves with its declaration, use runs kept apart around a removal, a
facade and two adjacent removals, and the refusal of an attributed declaration by `check` and `apply`. The unit tests sit beside the code they pin.

`tests/extract_facade_acceptance.rs` (live rust-analyzer) covers an `extract_module` glob over documented
`pub` items that `lib.rs` re-exports, a named extraction that keeps an unreferenced `pub fn` reachable
through the old path, and one that gates the names only the parent's tests use. The pure text rules
(`facade/facade_tests.rs`, `seam_survey/seam_survey_tests.rs`) are unit tests beside the code.
