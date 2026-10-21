# `move_impl_members`: moving an `impl`'s members to another module of the same crate

How the operation that lands a run of inherent `impl` members in another module is put together.
Behaviour is in the [feature doc](../../../docs/ft/coder/rust-code-restructuring.md#move_impl_members);
this page is where the code is, how a run flows, and what its limits are.

## Shape

rust-analyzer has no assist for this, and `extract_module` can only write a new child module of the file
and widens every private member it lifts to `pub(crate)`. Like `move_item`, the operation is authored here
and informed by the server: the outline says where the block and its members are, and
`textDocument/references` says who names each member or root item. Everything else is a function of the
files' text, in `assemble`.

`backends/rust.rs` holds one `SUPPORTED` entry and the two arms of `check` and `resolve_opening` that fold
the three same-crate kinds (`move_item`, `reparent_module`, `move_impl_members`) into
`same_crate_dispatch.rs`. The plan's own rules (P1, P6–P9) are `plan/codec/impl_move_fields.rs`.

## `backends/rust/impl_move/`

| Module | Decides |
|---|---|
| `impl_move.rs` | the run: `findings` (P2–P5, from the plan and the text), then read the origin, survey, refuse a bound name, assemble, and turn the files into one `FileEdit` each (a `Create` first for a created module) |
| `preflight.rs` | an item in the run that is not a member (S4), and a name the moved lines reach that the destination binds to a different item (S5): one it declares, or one a `use` there brings in by a path other than `crate::<origin>::<name>` |
| `survey.rs`, `survey/stayed.rs` | what the moved lines name in the origin (R4, confirmed by `references`) and the widenings (R6): moved members, stayed members and fields of the type in the origin and in each ancestor strictly below the common ancestor of origin and destination, and reached root items. A destination inside the origin's subtree makes the stayed and reached parts empty with no request. The widening is `item_move::members::widen_members` |
| `cut.rs` | the origin's cut (R2): the run's lines, plus one adjoining blank line, or the whole block when the run empties it; and the split of the widening edits into those inside the moved lines and those outside |
| `landing.rs` | the join rule (R3): the destination's one inherent `impl` token-equal in attributes and header, else a new block placed by `item_move::placement::insertion`; re-indentation leaves the bytes of a multi-line string literal alone |
| `assemble.rs` | the new text of every file the move touches, a function of texts: cut, landing, imports (`item_move::imports::needed`), relative spellings (`rebase::edits`), the created module's declaration, and the ancestors' widenings |

The shared readers are `retarget_impl::outline::{read, Run}`, widened to `pub(in crate::backends::rust)`,
with the operation's name in their S1/S2 wording, and `item_move`'s imports, placement, creation, rebase
and member widening. The dependency is one way: `item_move` and `retarget_impl` never name `impl_move`.

## Limits

- An ancestor's members are read from its file's top-level outline, so a block of the type inside an inline
  `mod` of an ancestor is not surveyed. A private member there that the moved lines name stays private and
  the compile gate refuses the run.
- The reached-item widening is written here over items, and `item_move/assemble.rs` keeps its own copy over
  its whole move.
- A name the destination imports by a different spelling of the same path (`super::step`) is refused as
  bound elsewhere. It over-refuses rather than binding silently.
- A tuple field read through `self.0` from outside the struct's module is not widened; the compile gate
  names it.

## `verify`

A member run out of a generic block that opens a new block repeats the generic header, its `where` and its
predicate lines, which `verify` reads as statements. `verify` excuses a gained copy of a whole generic header the
ref already holds, once per copy, with no declaration (`verify/retarget.rs`, `repeated_generic_headers`). A gained
`where` or predicate with no whole header beside it stays reported.

## Rule codes in the source

Comments in `impl_move/` name the rules by letter. **R1** the run is read with `retarget_impl::outline::read`;
**R2** the cut; **R3** landing (join or new block); **R4** imports; **R5** relative spellings; **R6** widening
(a moved members, b stayed members, c fields, d reached root items); **R7** callers are untouched. **P1–P9** are
the refusals made from the plan and the text alone (a missing or foreign `to`, a missing, origin or taken
destination, a `range`/`symbol` anchor, a whole-block anchor, a trait member path, a field the operation cannot
honour). **S1–S6** are the refusals made once the server has answered (two blocks, a cut member, a trait-impl
member, a non-member in the run, a name bound elsewhere, a one-line inline destination).
