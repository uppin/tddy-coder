# 2026-10-09 — a `pub use` chain that re-exports a move's destination is not followed when widening (needs a reproduction)

**Category:** Known limitation (engine capability), unconfirmed
**Source:** #reshape 1/19 (`widen-same-crate`), split out of
`2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle.md` (its second item)

## What the entry said

`move_item` widens the `mod` declarations on the path to the destination
(`packages/tddy-code-restructuring/src/backends/rust/item_move/reach.rs:103-140`, `widen_declarations`, which
follows `mod` declarations only through `destination::find_module`). A `pub use` chain that re-exports the
destination under another name is not followed.

## Why it is deferred

- No run has hit it: the lifecycle moves that recorded it say "none of which that run hit".
- It's not clear which caller would pass through such a chain. With `reexport: none`/`outside`, re-pointed callers
  are written with the canonical `crate::<destination>` path (`item_move/assemble.rs:310-317`), which goes
  through `mod` declarations only. With `glob`/`named`, the facade is written in the source module and also
  reaches the destination by its canonical path.
- The developer decided on 2026-10-09 (`#reshape` planning) to defer it until it can be reproduced.

## What would make it plannable

A fixture where a same-crate `move_item` applies and the compile gate then fails with `E0603`/`E0364` at a
path that goes through a `pub use … as …` of the destination (or one of its ancestors). Write that fixture
first. If no such fixture exists, delete this entry.
