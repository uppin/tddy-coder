# 2026-10-09 — `move_test_binary_to_crate` refuses a grouped or glob `use` of the origin

**Category:** Restructure engine limit (test-binary moves)
**Source:** #reshape 8/19 (`feature/reshape/move-grouped-use`), deferred from its scope

## What the engine does

`crate_move/test_binary.rs` `defining_home` (lines 801-822 on 2026-10-09) refuses a test binary path whose first segment is the
origin crate and whose second is a group or a glob (`use tddy_daemon::{a, b};`, `use tddy_daemon::*;`): "reaches several modules
of `<origin>` at once … write one `use` per path". The test-binary reader is text-only and has no path survey, so the group rule
`#reshape` 8 gave module and cluster moves (`crate_move::use_group`, Rule P / Rule S) does not reach it.

## Why deferred

No `#carve` or `#reshape` run has met this shape; every recorded refusal was a module or cluster move. `test_binary.rs` is being
split by `#reshape` 15 (`oversized-files`), so a feature edit there in the same wave would collide textually. Once the lexical
scanner is shared, each member of the group can be resolved with `crate_past_the_facades` and the statement rewritten through
`use_group::split_or_reprefix` (a glob stays refused: it has no member list to resolve).

## What would close it

A grouped origin `use` in a moved test binary is re-pointed per member: Rule P when every member has the same defining crate,
otherwise Rule S. Each defining crate goes to the destination's `[dev-dependencies]`.
