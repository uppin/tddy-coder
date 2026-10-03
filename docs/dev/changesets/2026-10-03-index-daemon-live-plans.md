# 2026-10-03 — Live plans: every loaded plan stays current as the tree moves; the carve the stack deferred

**Type:** Feature

`#live-plan` 7/15 — PR [#539](https://github.com/uppin/tddy-coder/pull/539),
`feature/live-plan/live-plans`, base `master` (the parents #537–#543 are merged). Product entry:
[2026-10-03-index-daemon-live-plans.md](../../ft/coder/changelog/2026-10-03-index-daemon-live-plans.md).

| Package | Entry |
|---|---|
| `tddy-code-restructuring` | [index-daemon-live-plans](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-03-index-daemon-live-plans.md) |
| `tddy-index-daemon` | [index-daemon-live-plans](../../../packages/tddy-index-daemon/docs/changesets/2026-10-03-index-daemon-live-plans.md) |
| `tddy-tools` | [index-daemon-live-plans](../../../packages/tddy-tools/docs/changesets/2026-10-03-index-daemon-live-plans.md) |

## What changed

**Live plans.** A carve is several plans applied in sequence under one root, and the first one's edits
moved everything the others anchor into. `PlanStore::fold_foreign_op` folds an operation applied from one
plan into every *other* loaded plan of the root: ranges are translated through the edit's line deltas, an
edit overlapping an anchored range marks that operation stale `EditedBy { plan, op }` and is never
translated, and file hints follow renames and moves. `reresolve_files` re-resolves loaded plans' item
anchors for files changed underneath the daemon (the daemon's `plan_upkeep` skips the files its own runs
wrote): a changed item is `ItemChanged`, a vanished one `ItemNotFound`. Stale operations are reported on
`ListPlans`, `PlanStatus` (proto `StaleOp`) and as `Check` findings, and `Apply` refuses
(`RestructureError::StaleOperation`, `FailedPrecondition`) any stale operation at or after the run's start,
before any write, honouring `--stop-after` and `--dry-run`. `restructure snapshot` of an item-anchored plan
re-resolves through `rebase_plan_file`. Per-file v2 hints (`sha256`, `modified`) are rewritten on a refresh.
One renderer, `console::stale_operations`, serves the in-process CLI, the daemon and `tddy-tools`.
Staleness is derived and held in memory for the life of a load; an unloaded plan is never touched, and is
judged at apply by the resolver's fingerprint refusal (item anchors) — a v2 plan has always reported
drift without refusing.

**Restructure tooling**, found by running the engine on real splits:
the apply tidy (unused imports removed, test-only imports gated, `rustfmt` over every file written);
`check --budget` counting production lines; one `--items` grammar (`parse_item_list`); `verify` comparing
logical statements and excusing, counting and summarising what an `extract_module` always causes;
`extract_module` carrying prelude-shadowing `use` names, keeping a widened item's type widened, rebasing
relative visibility one level deeper and re-rooting inline `super::` / `self::` paths; per-token progress
throttling with an unthrottled structured warm stream; non-ASCII-safe masking in the scanners;
`TDDY_INDEX_DAEMON_BIN` for `./run-index-daemon`.

**The carve.** The oversized files this stack deferred were split with the restructure engine, by engine
moves only.

## Final measurements (production lines: before the first `#[cfg(test)]` that opens a `mod`)

| File | Before | After |
|---|---:|---:|
| `plan_store.rs` | 522 | 477 |
| `plan.rs` | 887 | 377 |
| `runner/entry_points.rs` | 814 | 210 |
| `crate_move/moving.rs` | 594 | 301 |
| `backends/rust/imports.rs` | 664 | 396 |
| `backends/rust/early_return.rs` | 531 | 298 |
| `crate_move/cluster.rs` | 625 | 260 |
| `crate_move/source_scan.rs` | 527 | 326 |
| `verify.rs` | 618 (grown by this PR's own tooling work, then split) | 199 |
| `backends/rust.rs` | 4,475 | 2,666 — partly closed; the impl-member seams remain |
| `crate_move/test_binary.rs` | 966 | 967 — untouched but for a two-line non-ASCII fix |

## Backlog entries resolved (deleted at this wrap)

- `2026-10-02-split-oversized-plan-store-rs` — Split `plan_store.rs` (522 production lines, budget 500).
- `2026-10-02-split-oversized-plan-rs` — Split `plan.rs` (887 production lines, budget 500).
- `2026-10-02-split-oversized-runner-entry-points-rs` — Split `runner/entry_points.rs` (814 production lines, budget 500).
- `2026-10-02-cluster-rs-is-617-production-lines` — `crate_move/cluster.rs` is 617 production lines (also covered `source_scan.rs`, 527).
- `2026-10-02-crate-move-moving-rs-is-594-production-lines` — `crate_move/moving.rs` is 594 production lines after #541.
- `2026-09-18-restructure-verify-cannot-exit-zero-for-an-extract-module` — `restructure verify` cannot exit zero for an `extract_module`.
- `2026-10-03-restructure-tidy-gives-up-on-the-wide-facade-groups-a-split-leaves` — the apply tidy gives up on the wide facade groups a split leaves, and fails the run (this PR's own, fixed here).

Kept, narrowed before the wrap: `2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan` (item-anchored
plans are re-resolved by `restructure snapshot`; a v1 plan with `range` anchors still is not), `2026-09-24-restructure-apply-leaves-the-lint-gate-red` (the tidy closes
the unused-import and formatting shapes) and `2026-09-16-backends-rust-rs-is-4500-production-lines`
(the free-item runs are done; the impl-member seams remain). Filed by this PR:
`2026-10-03-live-plans-three-gaps-in-staleness-reporting-and-snapshot-routing` and
`2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass`.

## Code issues, final measurements

Deleted (`packages/tddy-code-restructuring/docs/code-issues/`):

| Record | Before | After |
|---|---:|---:|
| `oversized-file-plan-store` | 522 | 477 |
| `oversized-file-plan` | 887 | 377 |
| `oversized-file-runner-entry-points` | 814 | 210 |
| `oversized-file-crate-move-moving` | 594 | 301 |
| `oversized-file-backends-rust-imports` | 664 | 396 |
| `oversized-file-backends-rust-early-return` | 531 | 298 |

Kept: `oversized-file-backends-rust` (4,475 to 2,666, partly fixed, narrowed to the impl-member seams),
`oversized-file-test-binary` (966 to 967, unchanged in kind), `complexity-rust-facade-lines` (the function
moved to `backends/rust/facade.rs`), `dead-code-plan-filehint-modified` (still written, never read; its
location updated).

## Root causes worth keeping

- **The tidy failed a correct split because of overlapping per-unit spans and a one-shot repair.** A
  library checked with `--all-targets` is built twice, and each unit words its removal of one `use` group
  its own way, so the compiler's member spans overlap. `apply_fixes` kept an edit only while it did not
  overlap and skipped the rest without a word, so one name of a wide group stayed un-gated and the first
  re-check never quoted it; the redo then removed every other name some unit reported, including that one,
  and a redone round was never repaired a second time. The fix composes the units per statement from the
  *sets* of names, fails loudly on an overlapping edit, and repeats the repair while it improves.
- **A prelude-shadowed name is never reported unresolved.** A parent that binds `Result` to the crate's
  alias gives the moved code the alias; in the child the prelude's `std::result::Result` resolves the same
  word, so the import pass, which reacts to unresolved names, never sees it and the child silently means a
  different type. It needs a lexical pass over the parent's `use` declarations.
- **Relative visibility and inline `super::` paths change meaning one module deeper.** `pub(super)` and
  `super::f()` written in the parent mean something else in the new child; restoring the visibility *text*
  is only right for the absolute forms, and rust-analyzer's assist rewrites neither.
- **The progress throttle was in the wrong layer.** Throttling in `ServerChatter` suppressed phases for the
  daemon's structured warm stream, where a client cannot ask for a percentage it was never sent. The
  default now throttles the printed console and the warm stream builds `unthrottled()`.
- **Overlap is stale, not translated** — an edit inside another plan's range could have removed what it
  names, so refusing is the only safe answer; and **staleness is not serialised**, so it does not outlive
  a load (a gap, filed).

## Verification

CI on head `d5aec8db`: green — Rust tests 8315/8315, Web tests 2760/2760, build, arm64, lint, generated
code. A validation review found no boundary breach; its findings were fixed on the PR (the stale-operation
refusal honouring `--stop-after` and `--dry-run`, the journal-digest ordering, missing tests) or filed
(items 4–8 of the three-gaps backlog entry).
