# Same-crate moves and `retarget_impl` read a `use` the way rustc resolves it - PRD

**Date**: 2026-10-09
**PRD Type**: Bug fix (engine correctness)
**Status**: approved 2026-10-09, with the recommendations F1–F5 of the planning review (visibility read
from the scanner; R8 refuses; one hop; the stale marker removed; `reparent_module`'s clash check deferred)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `## Rust operations (v1)`
  › Same-crate moves (**Imports**, **What a caller keeps, and what the moved text says**) and
  › `retarget_impl` (S6); `## Known limitations` (the `move_item` `use`-header bullet loses its alias and
  glob sentences, which also misdescribe the alias defect).

No other feature document changes. No new operation, plan field, flag or wire message.

## Summary

Three ways the engine misreads a `use` stop. A relative path in moved code that goes through a module's
import (`super::service_util::f()` where the parent has `pub use tddy_session_split::service_util;`) is
kept as written when it still resolves from the destination, and otherwise followed to what the import
brings in — an item of this crate, an item of another crate, through an alias or through a glob — never
spelled as `super::<crate>::…`. A destination that imports the moving item under an alias keeps the alias.
`retarget_impl` treats `use super::m::T;`, `use self::m::T;` and `use m::T;` as the same binding as
`use crate::m::T;` and stops refusing them as a clash (S6).

## Background

- **`#carve` 21/21 (PR #536)**: three `move_item` lines moved `impl DaemonSessionHost` blocks into a new
  sibling module. `check --deep` reported nothing; the apply rewrote `super::service_util::…` to
  `super::tddy_session_split::service_util::…` (`E0433`) and the gate failed. Root cause: the engine reads
  every `use` head that is not `crate`/`self`/`super` as relative to the importing module, so the extern
  crate `tddy_session_split` became a child module of `connection_service`. It also follows the import
  even though the path as written already resolved from the destination (a child of the importing
  module, where its imports are visible). Hand-fixed by restoring the original spelling.
- **`#carve` 20/21**: `retarget_impl` was refused because the file already held
  `use super::launch_ports::LaunchSessions;` — the same item as the `use crate::…::LaunchSessions;` it
  would add, compared as text. Hand-fixed with consent (`TODO(restructure-retarget-impl-s6)` in
  `packages/tddy-agent-launch/src/svc_resume_claude_cli_session.rs`).
- **Same-crate limits (2026-10-05)**: a `super::Name` through an aliased or glob import of the module it
  names is not followed. The entry also says an aliased import of the moving item in the destination
  "reads as a name clash"; the code does not refuse it — it **drops** the aliased import and the gate
  fails on the now-unbound alias. The PRD fixes the real behaviour and corrects the description.

Each defect costs a failed apply or a refusal followed by a hand edit, on the plans the `#reshape` and
later crate-split stacks will run most (impl blocks behind facades of carved crates).

## Proposed Changes

### What's Changing

**One reading of a `use` path (Rust 2018).** A head of `crate`, `self` or `super` is resolved against the
module that writes the `use`. Any other head is a local name when that module binds it (a `mod` it
declares, an item it defines, a name it imports) and an **extern crate** otherwise. The result says which:
a path in this crate, or a path rooted in another crate. `move_item`'s clash check, `reparent_module`'s and
`move_item`'s relative-path rebasing, and `retarget_impl`'s S6 use this one reading.

**A relative path through an import (`move_item`, `reparent_module`).** For each `self::`/`super::` path in
moved code whose last step reaches module `M` and names `Name`, in this order:

| # | `M` and the destination | Written |
|---|---|---|
| R1 | `M` defines `Name` or declares it as a child module | the path to `M` from the destination (today) |
| R2 | the destination is `M` or inside `M` | the path to `M` — what was written, respelled only if the `super::` count changes; `M`'s imports are visible below it |
| R3 | `M` binds `Name` by an import whose visibility reaches the destination (`pub`, `pub(crate)`, `pub(super)`/`pub(in …)` covering it) | the path to `M` — a facade is the path the author wants named |
| R4 | `M` binds `Name` by a private plain `use p::Name;` of this crate | the path to `p` (today) |
| R5 | … a private aliased `use p::Real as Name;` | the path to `p`, and the name `Name` rewritten to `Real` |
| R6 | … a private `use p::*;` of this crate, with `p` binding `Name` | the path to `p`; exactly one glob of `M` must confirm the name |
| R7 | … a private `use` whose head is an extern crate (`use kernel::util;`) | the extern path, `kernel::util::`; `::kernel::util::` when the destination module itself binds the name `kernel` |
| R8 | `M` binds `Name` only through globs none of which (or more than one of which) confirm it | **refused**: the file, line, the path as written and why; nothing written |

R2 alone closes the `#carve` 21/21 miswrite (the destination was a sibling, so `super::service_util::` is
left byte-identical); R3 and R7 close the general case. R8 replaces a write that today is a certain
`E0603`.

**An aliased import of the moving item in the destination.** `use crate::pairing::peer as check;` in the
module `peer` moves into is rewritten to `use self::peer as check;` (plain or as a group member), not
removed. An un-renamed import of it is still removed (the destination now defines the name).

**`retarget_impl` S6.** Each top-level `use` leaf of the file that binds the new type's name is resolved
against the file's module; when it lands on the new type's path (`crate::<module>::<Name>`), the file
already binds it: no second `use` is written and nothing is refused. A leaf that resolves elsewhere —
another item, or another crate — is still refused as `E0255`, with today's message.

### What's Staying the Same

- The clash check's verdicts for every case it already decides correctly (a different item bound under
  the moved name, a destination import through a glob of the source module).
- `canonical_paths: true` and its notes; `repoint_facade_imports`; the cross-crate moves and
  `crate_move::reexports` (its own 2018 rule is not touched).
- S6 still runs after the server starts: a static `check` does not report it (as today).
- No widening of fields, `impl` members or a re-parented tree (node `widen-same-crate`); the tidy that
  skipped after the failed gate (node `tidy-facades`); `UseLeaf`'s other readers.
- No plan schema change; existing plans that resolved before resolve to the same edits, except that R2/R3
  stop rewriting a `super::` path that already reached what it named (fewer, not different, edits).

## Impact Analysis

### Technical Impact
- `tddy-code-restructuring` only: `item_move/{preflight,bindings,rebase,sites}.rs`,
  `retarget_impl/imports.rs`, the call sites in `item_move/assemble.rs` and `module_reparent/assemble.rs`,
  and the `use` scanner (`crate_move/source_scan`) gains each leaf's visibility text.
- One new live test binary (`tests/move_item_paths_acceptance.rs`), registered in `.config/rust-e2e.filterset`
  and the `rust-analyzer` group in `.config/nextest.toml`; the `retarget_impl` live cases join the already
  registered `tests/retarget_impl_acceptance.rs`. The rule table, the `use`-path reading and S6 are pinned at
  unit level without a server. (Registering the older unregistered same-crate suites is
  `feature/reshape/widen-same-crate`'s, per the 2026-10-09 developer decisions.)
- Functions on the `#reshape` function-size list (`path_edit`, `rewrite_statement`, `moved_text`,
  `items_of_module`) do not grow: new logic goes in new functions.
- One comment-only edit outside the crate: the stale `TODO(restructure-retarget-impl-s6)` marker in
  `packages/tddy-agent-launch/src/svc_resume_claude_cli_session.rs`.

### User Impact
- A move of code behind a facade of a carved crate applies instead of failing its gate; a `retarget_impl`
  in a file that imports the target relatively applies instead of being refused. No breaking change; the
  only new refusal (R8) replaces an apply that could not compile.

## Implementation Plan

1. The 2018 `use`-path reading (in-crate vs extern) and the scanner's visibility text; unit tests.
2. `retarget_impl` S6 over the resolved path; live tests.
3. The R1–R8 rule in `path_edit` behind a richer `imports` lookup (`bindings.rs`); unit tests per row.
4. The aliased destination import kept; unit + live test.
5. One live binary covering the `#carve` 21/21 shape (`an_app_over_a_kernel`), a private extern import,
   alias, glob and `reparent_module`; registration in the filterset and the test group.
6. Docs at wrap: feature doc (Imports, `retarget_impl`, Known limitations), `docs/same-crate-moves.md`,
   `docs/retarget-impl.md`; delete / narrow the claimed todos; drop the stale marker.

## Acceptance Criteria

- [ ] A `move_item` into a sibling of the module holding `pub use kernel::util;` leaves
      `super::util::f()` byte-identical and the workspace compiles with its tests
      ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] A `move_item` out of a module that privately imports `kernel::util` into a module outside it writes
      `kernel::util::f()` and compiles; the same holds for `reparent_module`
- [ ] A `super::Settings` through a private `use crate::types::Config as Settings;` arrives as a path to
      `types::Config`; a `super::Config` through a private `use crate::types::*;` arrives as a path to
      `types::Config`; both compile
- [ ] A `super::Name` through private globs that cannot confirm `Name` is refused naming file, line and
      path, by `check --deep` and by `apply`, with the tree untouched
- [ ] A destination importing the moving item as `use crate::pairing::peer as check;` keeps `check` bound
      and compiles
- [ ] `retarget_impl` in a file holding `use super::roster::Roster;` (and `self::`/child-relative
      spellings) applies with no second `use` and compiles; a binding of the name to a different item is
      still refused as `E0255`
- [ ] Unit tests pin every row R1–R8 and the in-crate/extern reading without a server
- [ ] The new live binary is in `.config/rust-e2e.filterset` and the `rust-analyzer` group
- [ ] tests pass for `tddy-code-restructuring` (`./test -p tddy-code-restructuring`, scoped; CI for the rest)

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-move-item-paths.md` (written after this PRD is reviewed)
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-move-item-paths-initial-discovery.md`
- Todo this closes: [2026-10-07 retarget_impl refuses a relative import of its target type](../../../dev/todo/2026-10-07-restructure-retarget-impl-refuses-a-relative-import-of-its-target-type.md)
- Todo this narrows (facade-path half; the tidy half is `feature/reshape/tidy-facades`'s): [2026-10-08 move_item miswrites a facade path in a moved impl block](../../../dev/todo/2026-10-08-restructure-move-item-miswrites-a-facade-path-in-a-moved-impl-block.md)
- Todo this narrows (alias and glob bullets): [2026-10-05 same-crate move limits](../../../dev/todo/2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle.md)
- Package docs: [same-crate moves](../../../../packages/tddy-code-restructuring/docs/same-crate-moves.md), [retarget_impl](../../../../packages/tddy-code-restructuring/docs/retarget-impl.md)
