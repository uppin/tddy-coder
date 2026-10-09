# `move_impl_members`: move methods of an `impl` into another module of the same crate - PRD

**Date**: 2026-10-09
**PRD Type**: Enhancement
**Stack**: `#reshape` 13/19 (`feature/reshape/move-impl-members`, base `feature/reshape/anchors-outline`; real parent `feature/reshape/widen-same-crate`)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md). Three sections change:
  `## Rust operations (v1)` (a new operation), `### Same-crate moves` (a third same-crate move beside
  `move_item` and `reparent_module`) and `## Known limitations` ("a member of an `impl` cannot move
  alone" closes).

No other feature document changes. The operation adds one `RefactorKind` and no new plan field: it
uses `to` and `name` with `move_item`'s meaning. It adds no flag and no wire message.

## Summary

A plan can now move a contiguous run of methods, associated functions and associated constants out of
one inherent `impl T` block and into an `impl T` block in **any other module of the same crate**. That
module can exist already, or the plan line can create it, as `move_item` does with `name`. The members
travel byte for byte, with their doc comments and attributes. Callers are not touched, because a method
is reached through its type wherever its block is written. Every private member, field or item that the
move puts out of reach is widened by as little as the code needs, and each widening is reported.

One plan line per run of members replaces what is impossible today: moving the members of
`impl RustBackend` out of `backends/rust.rs` (3,008 production lines) into its child modules.
`#reshape` 17 does exactly that.

## Background

`backends/rust.rs` has been past its 500-line budget for every PR of three stacks. The free-item runs
were carved out in `#live-plan` 7/15. What remains is the methods of three `impl RustBackend` blocks,
about 1,600 lines. The engine has two ways to move items, and neither moves a member somewhere useful:

- `move_item` refuses a range inside an `impl`. Its advice is to move the whole block.
- `extract_module` can lift a run of inherent members into a **new child module of the same file**
  (rust-analyzer writes `mod x { use super::T; impl T { … } }`). It cannot target an existing module, so
  #567's members could not join `signature_rewrites`. It widens every private member it moves to
  `pub(crate)` and never narrows one back, which the leftovers note counts as about 25 methods. And it
  depends on the assist's rename reaching every call. Where that rename misses, the run is refused with
  "an `impl` body cannot hold a `mod`". That refusal is what deferred the split twice (#567, `#sharpen`
  stack-overlap stops).

Rust needs none of what the assist does here. An inherent `impl` may be written in any module of the
type's crate, and a method call resolves through the type, so nothing that calls a moved member has to
change. The work is the same as `move_item`'s, done for members: cut the text, land it in a block of the
same type, write the imports the moved text needs, and widen what the split puts out of reach.
`#reshape` 1 (`widen-same-crate`) delivers the member widening. This node uses it and adds the operation
around it.

## Proposed Changes

### What's Changing

**New operation `move_impl_members`.**

- **Anchor.** An `items` anchor on members `c::m::Type::member` of one inherent `impl`, contiguous, as
  `restructure anchors <file> --items 'Type::a,Type::b'` emits it. A single `item` anchor on one member
  also works. This is the anchor `retarget_impl` already takes.
- **`to`** is the destination module, rooted at the package name. With **`name`**, `to` is the parent,
  and the module `name` is created in it first: `<parent dir>/<name>.rs`, declared with the narrowest
  visibility it needs, exactly as `move_item` does it.
- **The cut.** The run leaves the origin block, together with the doc comments and attributes attached to
  its first member. Comments that stand between the members travel with them. The origin block keeps the
  members before and after the run. A block the run empties is removed whole, with its header,
  attributes and doc comment.
- **Where the members land.** If the destination has exactly one inherent `impl` with the same header
  (generics, self type, `where` clause and attributes, compared token for token), the members are
  appended at its end. Otherwise a new block with the origin's header is written below the destination's
  last item. That is above a trailing `#[cfg(test)]` module, where `move_item` puts items too.
- **Imports.** The destination gets the origin module's `use` header, as `move_item` writes it, plus one
  `use` for each origin item the moved members name: the type itself, private helper functions, child
  modules. The end-of-run tidy prunes what is unused, in the origin as well.
- **Widening.**
  - A moved private member that is called from outside its new module is widened where it lands, to
    cover those callers. `fn start`, moved from `backends::rust` to `backends::rust::transport`, becomes
    `pub(super) fn start`.
  - A private member of `T` that stays and that the moved members call is widened only if the
    destination no longer sees it. The same holds for a private field of `T` and for a private item of
    the origin.
  - A destination inside the origin's subtree needs none of the last three widenings.
  - Each change is a report line: `` `RustBackend::start` private -> pub(super) ``.
- **Refused before a server starts**, from the plan alone, so `check` reports them:
  - `to` missing, or naming another crate;
  - a destination that does not exist (without `name`);
  - a `name` its parent already declares;
  - a destination that is the members' own module;
  - a `<Type>` block anchor (that is `move_item`);
  - a trait-impl member `<T as Tr>::m` (half of a trait `impl` cannot exist);
  - a range or symbol anchor;
  - any field the operation cannot honour: `reexport`, `canonical_paths`, `to_type`, `also`, `to_file`,
    `variant`, `expr`, `callee`, …
- **Refused when resolved** (`check --deep` and `apply`, before anything is written):
  - members in two `impl` blocks;
  - a member cut in half;
  - a non-member item inside the run, such as a macro invocation;
  - a member of a trait `impl`;
  - a moved member that names an origin item which the destination binds to a different item;
  - an inline destination written on one line.

### What's Staying the Same

- `move_item`, `reparent_module`, `extract_module` and `retarget_impl` behave exactly as before.
  `extract_module`'s own inherent-member path, and its `pub(crate)` widening, are untouched.
- No caller is edited. The operation leaves no facade, because no path can name a member.
- Trait impls move whole with `move_item`, as today.
- `restructure verify` needs no new rule. An `impl` header is not a statement it reads, and a leading
  `pub…` is excused. A test pins this.
- Not done, and recorded as known limitations:
  - a non-contiguous set of members (one line per run);
  - members of several blocks in one line;
  - a member that uses a `macro_rules!` defined earlier in the origin file;
  - widening of a private item of an origin **ancestor** other than the origin's own module, which is
    left to the compile gate (as `move_item` leaves it).

### Decisions (decided 2026-10-09; the developer approved every recommendation)

- **F1:** a new operation, not a new anchor shape for `move_item`.
- **F2:** a run that empties its block removes the block.
- **F3:** the members join the destination's one token-equal block, and open a new block otherwise.
- **F4:** widening surveys only the origin and its ancestors strictly below the common ancestor of origin and
  destination.
- **F5:** the three same-crate kinds share one dispatch arm in `check` and `resolve_opening`.
- **F6:** `canonical_paths` is refused.
- **F7:** `retarget_impl`'s member-run reader is shared in place: its visibility is widened and the operation name is
  passed in.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring` only. The new code goes in new files:
  - `backends/rust/impl_move.rs` and its children (preflight, landing, assembly as a function of texts,
    survey);
  - `plan/codec/impl_move_fields.rs`.
- Shared pieces are reused, not copied: `retarget_impl`'s member-run reader, `move_item`'s imports,
  placement, creation and rebase readers, and `#reshape` 1's member widening.
- `rust.rs` gains one `SUPPORTED` entry and no net lines in `check` or `resolve_opening`, which are both
  on `#reshape` 19's list (the same-crate arms are folded into one dispatcher).
- `item_move/assemble.rs` (507 lines) and its `visibilities` function do not grow. The reached-item half
  of `visibilities` is extracted by the engine so both operations can call it.
- Server cost per operation: one outline of the origin, one of the destination when it exists, and one
  `references` request per surveyed member, as `#reshape` 1 bounds it. A destination inside the
  origin's subtree adds nothing beyond the moved members' own survey.

### User Impact

- Splitting a file whose weight is in an `impl` becomes one plan line per run of members. That covers
  `rust.rs`, and the `cli_session_manager.rs`-style files the lifecycle carve met.
- Moved private methods come out `pub(super)`, or whatever the callers need, rather than `pub(crate)`.
- No breaking change: no existing plan line changes meaning.

## Implementation Plan

1. Plan surface: `RefactorKind::MoveImplMembers`, codec rules, plain-`check` findings, and the folded
   same-crate dispatch in `rust.rs`.
2. Member run: `retarget_impl`'s reader made shareable (visibility only, wording parameterised); the trait
   and block refusals.
3. Assembly as a function of texts: the cut, emptied-block removal, landing (join or new block), imports,
   rebase. Library tests, no server.
4. Widening: `#reshape` 1's member piece, plus the reached-item widening extracted from `visibilities`.
   The ancestor bound decides what is surveyed.
5. The live suite and its registration in `.config/rust-e2e.filterset` and the `rust-analyzer` group;
   `verify` pinned.
6. At wrap: the docs (feature doc, plan schema, package `same-crate-moves.md`), and the narrowing of
   leftovers § 1 and of the `rust.rs` record. Node 17 closes those.

## Acceptance Criteria

- [ ] a contiguous run of private and public methods of `impl T` moves into an existing module that has no block of `T`; a new `impl T` block holds them there; the members left behind still call them; the workspace compiles with its tests and lints clean
- [ ] the same run, moved into a destination holding exactly one `impl T` with the same header, is appended to that block
- [ ] `name` creates the destination module and the members land in it
- [ ] a moved private method called from the origin and from a sibling module becomes `pub(super)` (no wider), and the run reports it as `T::method`
- [ ] a move to a module outside the origin's subtree widens the private field and the private stayed method that the moved members use, and the origin's private helper function too; a move to a child module widens none of them
- [ ] a run covering every member of a block removes the block; a generic block `impl<'a> P<'a>` keeps its header where it lands
- [ ] every refusal listed above names the member or field and writes nothing; plain `check` reports the static ones
- [ ] `restructure verify` holds for the result
- [ ] tests pass for `tddy-code-restructuring` (scoped; CI for the rest)

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-move-impl-members.md`
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-move-impl-members-initial-discovery.md`
- Package design: [same-crate moves](../../../../packages/tddy-code-restructuring/docs/same-crate-moves.md)
- Todo entry narrowed here (item 1 of the leftovers todo; `feature/reshape/rust-backend-split` closes it):
  [leftovers of the live-plan carve](../../../dev/todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md)
- Parent that delivers the member widening: `feature/reshape/widen-same-crate`
- Successor that uses this operation: `feature/reshape/rust-backend-split`
