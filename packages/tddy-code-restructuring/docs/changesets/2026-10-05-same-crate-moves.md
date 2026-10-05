# 2026-10-05 — `move_item` and `reparent_module`, `reexport: outside`

**Type:** Feature

Two plan operations that move code inside one crate, a facade mode for them, and a repo-root hint on a
package-relative path. Product entry:
[2026-10-05-restructure-same-crate-moves.md](../../../../docs/ft/coder/changelog/2026-10-05-restructure-same-crate-moves.md).
How it works: [same-crate-moves.md](../same-crate-moves.md). The operations were first used for real on
`tddy-session-lifecycle`; that entry is
[2026-10-05-same-crate-moves-lifecycle-layout.md](../../../tddy-session-lifecycle/docs/changesets/2026-10-05-same-crate-moves-lifecycle-layout.md).

## What changed

- **`move_item`** (`backends/rust/item_move/`): a contiguous run of module-level items moves into another
  module of the same crate, from any file. `reexport` is `glob`, `named`, `none` or `outside`. A line that
  carries `name` creates the destination first (`item_move/creation.rs`): `to` is then its parent.
- **`reparent_module`** (`backends/rust/module_reparent/`): a module's file and directory move under
  another parent of the same crate with `git mv`; the `mod` declaration travels with its attributes and
  visibility. `reexport` is `glob`, `none` or `outside`; `named` and `name` are refused.
- **`reexport: outside`** (`item_move/outside.rs`): callers inside the library crate are re-pointed, and a
  facade is left only for what outside that crate reaches. A package's `tests/`, `examples/`, `benches/`
  and `src/bin` (beside a `src/lib.rs`) count as outside.
- **Plan codec and vocabulary:** `RefactorKind::{MoveItem, ReparentModule}`, `Reexport::Outside`,
  `Reexport::repoints_callers`; the codec rules are in [same-crate-moves.md](../same-crate-moves.md#plan-codec).
- **`crate_move/module_files.rs`:** the shared reading of the files a module spans.
- **Repo-root hint:** `owning_package`'s "is in no package" refusal names the repo-root path to write
  when the file exists below a package (`item_anchor.rs`).
- **`RestructureCommand::Warm`:** the client side is in `tddy-tools`; in process it is refused
  (`RestructureError::WarmNeedsIndexDaemon`). `console::snapshot_lines` and
  `item_anchor::plan_file_has_item_anchors` are shared with the daemon and `tddy-tools` for the snapshot
  routing.
- **Dispatch only in `backends/rust.rs`:** two `SUPPORTED` entries, two `check` arms, two `resolve` arms.

## Before and after

Production lines are counted to the first `#[cfg(test)]` that opens a `mod` (the `check --budget` rule),
measured at the merge base with `master` and at the branch tip. The 500-line budget for the files the
change grew is deferred by the developer; none of them was split.

| File | Before | After | Note |
|---|---:|---:|---|
| `backends/rust.rs` | 2,831 | 2,853 | +22: the dispatch and `check` arms, the two `mod` lines and two `SUPPORTED` entries; every line of logic is in the new module trees |
| `plan.rs` | 481 | 520 | the two kinds, `Reexport::Outside` and their docs; crossed 500 |
| `plan/codec.rs` | 437 | 514 | the destination, anchor and facade rules for the two operations; crossed 500 |
| `item_anchor.rs` | 463 | 517 | the repo-root hint, `plan_file_has_item_anchors`; crossed 500 |
| `backends/rust/facade.rs` `facade_lines` | 51 | 58 | fn line to closing brace: one match arm that refuses `Outside` for an extraction; nesting unchanged |
| new files | — | at most 429 lines each | `item_move/assemble.rs` is the largest, tests included; none is over 500 |

Tests recorded while the change was built, scoped to this package
(`cargo test --no-fail-fast -p tddy-code-restructuring -- --test-threads=1`): the baseline before any engine
change was 1,004 passed and 26 failed, the 26 being exactly the red tests of the two new suites and of
`anchors_package_relative_path`; after `reexport: outside` it was 1,108 passed and 0 failed, one doc test
ignored. Clippy `--all-targets -D warnings` and `cargo fmt --check` were clean for the package.

## Defects the lifecycle moves found, fixed in the engine

The moves of `tddy-session-lifecycle` were run through the new operations, and every refusal or compile
failure stopped them and was fixed here, test first, never by a hand edit:

1. `reexport: outside` counted a package's `tests/`, `examples/` and `benches/` (and a binary beside a
   library) as inside the crate.
2. `check --deep` refused every item-anchored `move_item` ("names no module-level item"): the preflight
   read the range the deep check had lowered the anchor to.
3. `extract_module` could not compose a topic module gathered from several files, because it cannot
   re-point callers: a `move_item` line that carries `name` creates its destination.
4. A later move into an existing module did not widen that module's `mod` declaration; copied imports of a
   module the destination cannot see broke the build; a group import was copied whole over a binding the
   destination already had; imports landed below the first item of an empty file; a facade and a re-pointed
   `use` bound one name twice in the source file.
5. The destination's own `use` of the moving item, also through a glob re-export, was counted as a name
   clash.
6. A re-parented module's `super::Name` was respelled through its old parent's private import of `Name`.

## Backlog

Resolved and deleted: `2026-10-04-restructure-extract-module-cannot-gather-items-from-several-files` and
`2026-10-04-restructure-anchors-and-snapshot-friction-seen-in-carve-16a`. Partly resolved and narrowed:
`2026-09-24-lifecycle-modules-to-re-parent-by-hand` and
`2026-10-03-live-plans-three-gaps-in-staleness-reporting-and-snapshot-routing` (its item 2).

Remaining, filed by this work: `2026-10-04-restructure-move-item-copies-the-whole-use-header`,
`2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members`,
`2026-10-04-restructure-reexport-outside-limits`,
`2026-10-04-restructure-reparent-module-does-not-widen-what-the-moved-tree-reaches`,
`2026-10-04-restructure-reparent-module-first-cut-limits` and
`2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle`. Constraints that held and still
bind: `2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node` (`rust.rs` took dispatch
lines only), `2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures`,
`2026-09-24-restructure-apply-leaves-the-lint-gate-red`,
`2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind`,
`2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling` and
`2026-10-03-restructure-glob-reexport-is-narrower-than-the-moved-items-need` (open).

## Code issues

| Record | Measurement |
|---|---|
| `oversized-file-backends-rust` | 2,831 to 2,853 production lines (+22, dispatch only); stays open |
| `complexity-rust-facade-lines` | 47 to 54 lines by the record's count (51 to 58 fn line to closing brace), nesting 5 unchanged, one match arm and one early exit added; stays open |
| `broken-restructure-anchors-empty-outline` | the claim by #537, merged on 2026-10-02, is removed and the remainder is unowned; a snapshot of an item-anchored plan no longer reaches the cold path when a daemon is configured, which narrows the exposure for `snapshot` only; `anchors` is as it was |
| `dead-code-plan-filehint-modified`, `oversized-file-test-binary` | unchanged (967 production lines for `test_binary.rs`; no read site of `modified`) |
