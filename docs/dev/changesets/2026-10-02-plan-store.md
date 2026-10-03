# 2026-10-02 — Plan store: plans are loaded once, executed by reference and flushed back

**Type:** Feature

`#live-plan` 2/7 — PR [#538](https://github.com/uppin/tddy-coder/pull/538),
`feature/live-plan/plan-store`, base `feature/live-plan/item-anchors` ([#537](https://github.com/uppin/tddy-coder/pull/537)).
Successors: [#539](https://github.com/uppin/tddy-coder/pull/539), [#540](https://github.com/uppin/tddy-coder/pull/540).
Product entry: [2026-10-02-plan-store.md](../../ft/coder/changelog/2026-10-02-plan-store.md).

`tddy-code-restructuring` gains `plan_store.rs`: a `PlanStore` per workspace root that loads plans once,
gives every operation a stable id, runs `Check`/`Apply`/`PlanStatus` from the loaded copy, refreshes the
applied plan's pending anchors after each operation and writes the plan back. `tddy-index-daemon` keeps
one store per root, adds `LoadPlans`/`UnloadPlans`/`ListPlans` and flushes on shutdown; `tddy-tools`
routes `restructure load|unload|plans`.

| Package | Entry |
|---|---|
| `tddy-code-restructuring` | [plan-store](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-02-plan-store.md) |
| `tddy-index-daemon` | [plan-store](../../../packages/tddy-index-daemon/docs/changesets/2026-10-02-plan-store.md) |
| `tddy-tools` | [plan-store](../../../packages/tddy-tools/docs/changesets/2026-10-02-plan-store.md) |

The skill `code-restructuring` retires "the plan is never rewritten" and documents load/check/apply.

## Decisions taken with the developer (2026-10-02)

- **The legacy-journal compatibility path is kept**: a journal from before write-back resuming a
  range/symbol plan runs unnumbered with no write-back. Consent given at wrap.
- **File-length gate, deferred with consent**: `plan.rs` (799 to 887 production lines) and
  `runner/entry_points.rs` (602 to 814) are not decomposed here; `plan_store.rs` (new, 522) waits for the
  stack because #539 also edits it. Follow-ups are in `docs/dev/todo/`
  (`2026-10-02-split-oversized-plan-rs`, `-runner-entry-points-rs`, `-plan-store-rs`).
- **`--from <id>` is not carried over the wire**; `ApplyRequest.from_op` would break every exhaustive
  literal in the daemon's acceptance suite. Journal and events carry `op_id`.

## Code issues, final measurements

| Record | Before | After | Disposition |
|---|---|---|---|
| `stale-repo-scoped-restructure-state-apply` (tddy-index-daemon) | `apply.rs:45` called `StatePaths::under(root)`; 4 doc comments and 1 package doc justified the queue by a journal with no plan identity | the daemon's apply loop opens its run through `runner::open_plan_run`, which uses `StatePaths::for_plan`; 0 calls of `StatePaths::under` in `tddy-index-daemon/src`; 0 occurrences of "no plan identity" / "no lock file" in its sources; the package doc reconciled | **Closed, record deleted** (claim #538 finished) |
| `oversized-file-plan` | 799 | **887** | Kept, regressed +88; deferral consented |
| `oversized-file-runner-entry-points` | 602 | **814** | Kept, regressed +212; deferral consented |
| `oversized-file-plan-store` | new | **522** | Created, open; deferred until the stack lands (#539 edits it) |
| `dead-code-plan-filehint-modified` | 0 read sites | 0 read sites | Unchanged; this PR touched `plan.rs` but not the field |

Other open records in the three packages (`oversized-file-backends-rust`, `oversized-file-test-binary`,
`complexity-rust-facade-lines`, `broken-restructure-anchors-empty-outline`, the index daemon's warm-latch
and `narrate_until_loaded` records, and the `tddy-tools` `server.rs` / `cli.rs` records) name code this
PR did not change and were left as they are.

## Verification

`./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools` (scoped, as the repo's
verification rule asks): **1110 passed, 0 failed, 9 ignored** (validation run of 2026-10-02, HEAD
`9b417d5a`; the wrap changed documentation only). The rest of the workspace is CI's.
