# 2026-09-19 — A cluster of modules moves as one unit, and run state is keyed by the plan

**Type:** Feature

`move_cluster_to_crate` moves a **set** of modules into another crate in a single edit. `anchor` is
the first member, `also` carries the rest, `to` and `reexport` behave as `move_module_to_crate`'s.
The whole set moves or none of it does, so the tree is never half-moved — and that is what makes a
mutually-referencing group movable at all. Moved one at a time, each module's reference to a sibling
still in the origin makes the destination depend on the crate it left, and no ordering of one-module
operations can resolve a cycle. A set of one is refused; that is `move_module_to_crate`.

A path reaching a **co-moving** member keeps `crate::` — the destination *is* `crate` for a file that
has arrived in it. Naming the destination crate there is `E0433` plus the self-dependency the
operation exists to avoid. A path reaching a module staying behind is re-pointed at the origin,
unchanged.

`check` names the sibling a **partial** set would strand, statically and with no rust-analyzer, and
applies every cross-crate precondition per member — so it keeps parity with `apply` for a cluster as
it does for a single move. `--budget` measures every member rather than only the anchor's file, and
the ledger translates every member's anchor, so an earlier rename follows through to all of them.

Run state is keyed by the **plan**, at `<root>/.restructure/<plan stem>-<digest>/`. A completed plan
no longer blocks the next one under the same root, and `--resume` resumes the plan it was given
rather than whichever ran last. Every multi-layer restructuring is several plans in one repository,
so this removes a per-layer hand-archiving step. A journal left at `<root>/.restructure/` by an older
run is adopted when resuming and otherwise refused by name — never silently taken over.

`crate_move.rs` was carved along its own seams first, from 1,340 production lines to 320, into
`crate_move/{moving,module_home,manifest_edits,header,refusals,cluster,destination,preconditions}.rs`.
The carve was behaviour-preserving and its own commit.

## Final measurements

Closing the standing code issue *blocking: `move_module_to_crate` cannot move the shapes this
workspace has*, whose record is deleted with this entry. All four refusals it tracked are closed —
two by [#488](https://github.com/uppin/tddy-coder/pull/488), two here:

| Run | Modules moved | Refusals standing |
|---|---|---|
| 2026-09-10 | 13 / 21, then 0 / ~24, then 0 / 4 entangled | 4 |
| 2026-09-18 (#488) | — | 2 |
| 2026-09-19 (this change) | **2 / 2 entangled, in one operation, `cargo check` clean** | **0** |

Measured live against a real rust-analyzer on a throwaway workspace whose `spawner` and
`spawn_worker` reference each other: `apply` reports `MoveClusterToCrate -> 4 file(s) applied`, both
headers read `use crate::…`, the destination manifest gains no self-dependency, and
`cargo check --workspace` passes. The same plan expressed as two `move_module_to_crate` ops still
refuses on the first, which is why the new op exists rather than grouping in the apply loop.

Tests: 387 passing across 15 targets, including a new live `cluster_move_acceptance` suite that ends
in `cargo check`. Single-module behaviour is unchanged, pinned by `move_module_to_crate_acceptance`,
`nested_module_move_acceptance` and `facade_cycle_acceptance`.

**Measure this crate with `--no-fail-fast`.** `./test` does not pass it, and `cluster_move` sorts
third of fifteen targets, so a failure there hides the twelve after it — including the three live
suites that are the only evidence single-module behaviour still holds.

## Resolved backlog entries

- `2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md` — deleted. Its § 1 (a
  mutually entangled cluster cannot be expressed) is what this change closes; its § 2
  (`--indexing-budget`) was already answered and the flag withdrawn.

`2026-09-09-restructure-defects-from-the-first-cross-crate-move.md` was **claimed for deletion and
is deliberately kept**, narrowed instead. Re-reading it at wrap showed it had accreted sections from
`#unbundle` nodes 2, 4 and 6 whose defects this change does not touch, and which exist nowhere else:
no vocabulary for a seam split (an item-list anchor, as opposed to whole co-moving modules); `git mv`
refusing an uncommitted file after already appending its `pub mod`; the `pub(crate)` widening a
cross-crate move forces going unreported; and a caller re-point spliced inside a grouped import. All
four were verified still absent from the code.

## Not covered by a test

AC5 end to end — two plans applied back to back under one root — is **reasoned, not pinned**. The
unit tests prove two plans resolve to different directories and that one plan's directory is stable,
which is what `--resume` needs; no test applies two plans in sequence, because that needs a git
worktree and a live server. Worth adding the next time this area is opened.

## Left open

One consumer still uses the repository-scoped layout — `tddy-index-daemon`'s apply loop — recorded
as `stale-repo-scoped-restructure-state-apply.md` in that package. The two cosmetic defects (one
`pub use <crate>::*;` per operation rather than per destination; `pub mod` lines appended out of
order) are re-filed as `docs/dev/todo/2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md`.
`extract_module` cannot see sibling seams cut by the same plan —
`docs/dev/todo/2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md`.
