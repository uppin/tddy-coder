# Same-crate moves widen what they split, and leave no empty directories - PRD

**Date**: 2026-10-09
**PRD Type**: Enhancement
**Stack**: `#reshape` 1/19 (`feature/reshape/widen-same-crate`, base `master`)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `### Same-crate moves`
  (the **Visibility** paragraph) and `## Known limitations` (four entries close, including "a rolled-back
  group leaves an empty directory", and two narrow).

No other feature document changes. No plan field, flag, operation or wire message is added: the
behaviour of existing `move_item` and `reparent_module` lines changes from "applied, then stopped by the
compile gate" to "applied and compiles".

## Summary

A same-crate move now widens everything it splits apart, not only module-level items, by as little as
the code needs, and reports each change:

- `move_item` widens a **private field or method** when the move puts it in one module and the code that
  uses it in another: a moved struct whose `impl` stayed, a moved `impl` reading a struct that stayed, a
  moved function calling a private method that stayed, and the reverse of each.
- `reparent_module` widens what the moved tree reaches: a **private item of the old parent** (or an
  ancestor) that the tree names, a **`pub(super)` / `pub(in …)`** item in the tree whose old meaning reached
  outside it, and it **respells an absolute `pub(in crate::…)`** that points into the tree.
- Any operation that moves files with `git mv` **removes the directories the move emptied**.

## Background

Both operations shipped in `#584` with item-level widening only. The gaps were recorded as known
limitations, and each one fails the same way: the edit is applied, the compile gate stops the run with
`AppliedTreeDoesNotCompile` (`E0616`, `E0624` or `E0603`), and the author pays a whole index and rolls the
edit back by hand. The engine has the information it needs. The outline already lists the fields and
methods under each struct and `impl`. The old parent is already open on the server. And `Scope`, which
reads a visibility keyword as the module subtree it covers, already answers "how wide is enough".

This stack's later nodes depend on it. `#reshape` 13 (`move-impl-members`) moves `impl` members
between modules and widens them the way this node does. Regrouping `backends/rust/` before the
`tddy-code-restructuring` crate split is a set of module moves through a crate that uses
`pub(in crate::backends::rust)` heavily (`reparent-module-does-not-widen…`, discovery item 5). The
emptied directories are a manual `rmdir` after every module move today, and every cross-crate move
leaves them too.

## Proposed Changes

### What's Changing

**`move_item`: fields and members split by the move.**

- **Which members are surveyed:** the named children of the root symbols in the outline. That means the
  fields of a struct, and the methods, associated functions and associated constants of an **inherent**
  `impl`. Two sets are surveyed. (a) Every private member of a struct or inherent `impl` that moves.
  (b) Every private member of a struct or inherent `impl` that stays, when the moved lines mention its
  name. This is the same name filter `reached_by_the_moved_code` uses for items. Each surveyed member costs
  one `textDocument/references` request, and the progress line counts them.
- **Which members are skipped:** members of a trait `impl` and items of a trait (they have no visibility
  of their own), enum variants, and anything already `pub` or `pub(crate)`.
- **The rule:** a member that moves is widened where it lands to cover the modules that use it from
  outside the moved lines. A member that stays is widened where it is written to cover the destination.
  This is the same `Scope::widened_to` fold the items get. A member whose users are all inside its new
  subtree is left as written: a destination that is a child of the source sees its parent's private
  fields, so no widening is needed there.
- **Report:** one line per member, qualified by its type: `` `Counter::count` private -> pub(crate) ``.
- **Shape:** the survey and the widening are one shared piece that takes the members of a type split
  across two modules and returns the visibility edits and report lines. `#reshape` 13 consumes it.

**`reparent_module`: what the moved tree reaches.**

- **Old parent and ancestors:** a private item (including a private `mod` declaration) of the old parent
  that the moved files name is widened to cover the module's new path. So is one of an ancestor strictly
  below the common ancestor of the old and new parents. An item of a module that is also an ancestor of
  the new location needs nothing. Candidates are the root items of those modules whose names the moved
  files mention, confirmed by `textDocument/references` landing in a moved file.
- **Visibilities written in the moved tree:** each `pub(…)` is read at its old module.
  - A scope that lies inside the tree is translated with it, so `pub(in crate::host::attachments)` becomes
    `pub(in crate::split::attachments)` and a child's `pub(super)` is untouched.
  - A scope that reaches outside the tree keeps everything it covered and is widened until it is legal
    where the file now sits. `pub(super) fn materialize` in `host::attachments`, moved under `split`,
    becomes `pub(crate)`.
  - No server request is needed: every caller that could see the item before still can.
- **Report:** each widening or respelling is a report line, in the same format as the declaration's.

**Every `git mv`: emptied directories.**

- After an operation's renames, `apply` removes each directory a renamed file left, and walks up its
  ancestors while they are empty, stopping at the workspace root.
- A directory that still holds anything is left as it is: a data file no `mod` reaches, a `.DS_Store`, or
  a directory that was not emptied by this operation.
- This covers `reparent_module`, the cross-crate moves and `move_test_binary_to_crate`. Rollback and re-run
  already recreate a directory before writing into it.
- A rolled-back transactional group that removes a file it created also removes the directories that removal
  emptied, by the same sweep (a known limitation today).

### What's Staying the Same

- Every refusal both operations make today, the facade forms, caller re-pointing and the import pass.
- `extract_module`'s own widening and narrowing (`backends/rust/visibility.rs`, `facade.rs`).
- Item-level widening for both operations, and the widening of the `mod` declaration.
- `check --deep` output. Widenings travel in the resolution's report, which `check --deep` does not print
  today. Printing them is `#reshape` 7's (`move-widen`) responsibility, and these widenings appear there
  once it lands.
- Cross-crate moves widen nothing new here. That is `#reshape` 7.
- Not done here, and left as known limitations:
  - tuple-struct fields (`self.0`), if the outline does not list them (verified in the first push). One todo
    covers this for same-crate and cross-crate moves, written by `#reshape` 7:
    `docs/dev/todo/2026-10-09-restructure-moves-do-not-widen-tuple-struct-fields.md`;
  - a `pub use` chain that re-exports a destination under another name (no reproduction; deferred to its own
    todo);
  - an item of a *sibling* module that the moved tree reaches through `pub(super)` (only the old parent
    and the ancestors are surveyed);
  - the aliased-`use` and glob-`super::Name` clash-check items (`#reshape` 11);
  - the remaining first-cut refusals of both operations.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring` only. The new code goes in three new files: `item_move/members.rs`,
  `module_reparent/tree_reach.rs` and `module_reparent/tree_visibility.rs`. `apply.rs` gains a directory
  sweep. `item_move/assemble.rs` (507 lines, `#reshape` 15's target) gains only a call and a field, and
  `assemble::visibilities` (71 lines) does not grow.
- The server cost per run grows by one references request per surveyed member and per candidate ancestor
  item, plus one outline per surveyed ancestor file. Both are bounded by the filters above and counted on
  the progress line.
- The live tests go in one new suite, which is registered in `.config/rust-e2e.filterset` and the
  `rust-analyzer` test group. The same edit registers every live rust-analyzer suite of the package that is
  missing from either file today (12 missing from the filterset, 31 from the group), which is the
  developer's 2026-10-09 assignment to this node. The e2e leg runs those 31 one at a time from now on.

### User Impact

- A move that splits a struct from its `impl`, or reparents a module that reaches its parent's private
  items, applies and compiles instead of failing at the gate after a full index. The widening report
  shows what was opened up, so it can be reviewed.
- There is no `rmdir` after module moves.
- No breaking change: a plan that compiled before produces the same edit, plus any directory removal.

## Implementation Plan

1. Emptied-directory sweep in `apply.rs`, with unit tests (tempdir + git, the existing pattern).
2. `item_move` member survey: read the children from the outline, two member sets, the references.
3. Member widening as a function of texts (library tests, no server), including the field-aware
   visibility edit (today's `outline::visibility_edit` refuses a field) and the report line.
4. `reparent_module` tree visibilities: translate or widen each `pub(…)` in the moved files (library
   tests).
5. `reparent_module` reach into the old parent and ancestors: outline, mention filter, references, widening.
6. One live suite, and the registration of every live suite of the package. Closing the claimed todo entries and narrowing the partial ones
   happens at wrap, with the feature and package docs.

## Acceptance Criteria

- [ ] `move_item` of a struct whose inherent `impl` stays, and of an `impl` whose struct stays, widens the private field the other half reads; the run reports `Type::field`; the workspace compiles with its tests and lints clean
- [ ] `move_item` widens a private method of a left-behind `impl` that the moved code calls, and a private method of a moved `impl` that the left-behind code calls (formerly `E0624`)
- [ ] `move_item` into a child of the source module widens no member, and a trait `impl` member is never touched
- [ ] `reparent_module` widens a private item of the old parent that the moved tree names (formerly `E0603`), and no further than the module's new path needs
- [ ] `reparent_module` widens a `pub(super)` item of the moved module that the old parent names, and respells an absolute `pub(in crate::<old tree>)` to the new tree path; a child's `pub(super)` inside the tree is unchanged
- [ ] after a `reparent_module`, the directories the move emptied are gone, and a directory still holding an unreached file is kept
- [ ] a rolled-back or re-run operation still finds the directories it writes into, and a rolled-back group leaves no directory it emptied
- [ ] every test binary of `tddy-code-restructuring` that drives a live rust-analyzer is in `.config/rust-e2e.filterset` and the `rust-analyzer` test group
- [ ] tests pass for `tddy-code-restructuring` (scoped; CI for the rest)

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-widen-same-crate.md` (written after this PRD is reviewed)
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-widen-same-crate-initial-discovery.md`
- Package design: [same-crate moves](../../../../packages/tddy-code-restructuring/docs/same-crate-moves.md)
- Todo entries this resolves:
  - [move_item does not widen fields or impl members](../../../dev/todo/2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members.md)
    (the main limit; its "other limits of the first cut" list stays, so the entry is narrowed)
  - [reparent_module does not widen what the moved tree reaches](../../../dev/todo/2026-10-04-restructure-reparent-module-does-not-widen-what-the-moved-tree-reaches.md)
- Todo entries narrowed at wrap:
  - [same-crate moves limits found moving lifecycle](../../../dev/todo/2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle.md)
    (emptied directories close here)
  - [reparent_module first-cut limits](../../../dev/todo/2026-10-04-restructure-reparent-module-first-cut-limits.md)
    (emptied directories close here; the doc-wording and byte-identical-test items are already fixed on master and close too)
- Deferred to new todo entries: `2026-10-09-restructure-pub-use-chain-widening-needs-a-reproduction.md`,
  `2026-10-09-restructure-reparent-does-not-widen-sibling-module-items-the-tree-reaches.md`; tuple fields:
  `2026-10-09-restructure-moves-do-not-widen-tuple-struct-fields.md` (`#reshape` 7)
- Successor that consumes the member widening: `feature/reshape/move-impl-members`
