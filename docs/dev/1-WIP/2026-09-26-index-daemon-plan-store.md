# Changeset: Index daemon plan store — plans are loaded once, executed by reference, and flushed back

**Date**: 2026-09-26
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-26-index-daemon-plan-store-initial-discovery.md).

## Stack

`#live-plan` 2/7 — branch `feature/live-plan/plan-store`, base `feature/live-plan/item-anchors`.
PR: [#538](https://github.com/uppin/tddy-coder/pull/538)

## Responsibility

- Stable op `id`s in the plan: assignment on load, duplicate refusal, written back on flush; journal,
  events and `--from` naming ops by id.
- `PlanStore` (library, `tddy-code-restructuring`): load / unload / unload-all / list / lookup by
  `(plan, op id)`, keyed by workspace-relative plan path under one root.
- Executing `Check`, `Apply`, `PlanStatus` from the store; implicit load on `Apply`.
- Refreshing **the applied plan's own** pending ops after each operation (hints, relative ranges,
  fingerprints of edited items, `file` hints through renames).
- Flush: dirty tracking, eventual flush, synchronous flush at run end / unload / shutdown, atomic
  write, refusal to clobber a plan changed on disk since load.
- `LoadPlans` / `UnloadPlans` / `ListPlans` RPCs; `restructure load` / `unload` / `plans` on both
  command lines; the in-process path's per-invocation store.
- Plan-scoped run state in the daemon's apply loop.

## Boundaries

- Does **not** refresh any plan other than the one being applied, and does not react to tree changes
  the daemon did not make — `live-plans`.
- Does **not** detect or report stale ops.
- Does **not** change `restructure snapshot`.
- Does **not** change the anchor kinds, the resolver or the v2 header.

Run-open handling of item anchors for **continued** runs moved into this PR (it was the parent's
deferral): `ItemAnchorsOnContinuedRun` and `refuse_a_continued_item_plan` are gone, replaced by the
journal check in `runner/resume.rs`. The resolver and the anchor kinds are untouched.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `item-anchors` (1/7) | `Anchor::{Item, Items}`, `ItemPath`, `Fingerprint`, the v2 header in `plan.rs`; `resolve_item` in `backends/rust/item_path.rs`; run-open `resolve_item_anchors` in `runner` | the store keeps item anchors as parsed; the per-op refresh recomputes fingerprints and relative ranges by calling `resolve_item` on the post-op tree | add an anchor kind, change `ItemPath` syntax, change the resolver or its refusals, change the header |

## Draft PR contract

The first push after this commit (wave 2) publishes:

- `plan.rs`: `RefactorOp.id: Option<OpId>`, `Plan::to_jsonl(&self) -> String`.
- `plan_store.rs` (new): `PlanStore`, `LoadedPlan`, `PlanKey`, `OpId`, `FlushPolicy`,
  `PlanStore::{load, unload, unload_all, list, get, op, refresh_after_op, flush_dirty, flush_all}` —
  bodies `TODO(plan-store): implement`.
- `code_index.proto`: `LoadPlans`, `UnloadPlans`, `ListPlans` with their messages; service methods
  returning `unimplemented` until green.
- `restructure_args.rs`: `load`, `unload`, `plans` subcommands.
- The failing acceptance and unit tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — `the_applied_plans_pending_op_follows_an_edit_inside_its_item`
and the fingerprint refresh need `item-anchors`' resolver to behave; greenable as soon as
`item-anchors` is green. Every other test uses v1 anchors or the store alone.
**Concurrent with:** `check-parity`
**Blocks:** `live-plans` (it extends this store's refresh to every loaded plan)

    item-anchors → plan-store → live-plans      move-paths → check-parity

## Successor PRs

- `feature/live-plan/live-plans` — every loaded plan kept current; stale ops.

## Prerequisites

### ✅ RESOLVED HERE — the apply loop opts out of plan-scoped restructure state — [`stale-repo-scoped-restructure-state-apply.md`](../../../packages/tddy-index-daemon/docs/code-issues/stale-repo-scoped-restructure-state-apply.md)

The store is keyed by plan, so the daemon's run state must be too; the per-root queue's doc comments
that justify it by "the journal carries no plan identity" are reconciled here. This node's wrap
deletes the record.

### ℹ REFERENCE — `extract_module` cannot see sibling seams cut by the same plan — [`2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md`](../todo/2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md)

A plan-level projected module tree would live naturally beside the store, but it is a separate
design and is left open. The entry stays.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md) —
  `plan_store`, op ids, `Plan::to_jsonl`, run by reference
- **tddy-index-daemon**: [README.md](../../../packages/tddy-index-daemon/README.md) — three RPCs,
  store per root, flush on shutdown, `StatePaths::for_plan`;
  [code-index-service.md](../../../packages/tddy-index-daemon/docs/code-index-service.md)
- **tddy-tools**: [README.md](../../../packages/tddy-tools/README.md) — `load` / `unload` / `plans`
- **Skill**: `.agents/skills/code-restructuring/` — load/check/apply loop; "never rewritten" retired

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-09-26-index-daemon-plan-store.md)
- [Warm code-intelligence daemon](../../ft/coder/warm-code-intelligence-daemon.md)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md)

## Summary

A library `PlanStore` holds parsed plans by path and op id; the daemon keeps one per root across
requests, a one-shot run keeps one for its own lifetime. Operations execute from the store, the
applied plan's pending ops are refreshed after each op, and changed plans are flushed back.

## Background

`apply.rs` re-parses the plan file per run; the plan-schema reference calls the plan a command log
that is never rewritten; the ledger's in-run corrections are lost at exit; the daemon's run state is
repo-scoped.

## Scope

- [ ] Op ids
- [ ] `PlanStore` with load / unload / list / lookup
- [ ] Check / Apply / PlanStatus from the store; implicit load
- [ ] Per-op refresh of the applied plan
- [ ] Flush policy, atomic write, clobber refusal, flush on shutdown
- [ ] Three RPCs, CLI subcommands (both front ends), no-daemon refusals
- [ ] Plan-scoped run state in the daemon

## Technical Changes

### State A (Current)

- `tddy-index-daemon/src/apply.rs:44` parses the file per run; `StatePaths::under(root)`.
- `index.rs`: per-root `WorkspaceIndex` with a queue, no plan state.
- Served mode stops on `^C`/`SIGTERM` (`serve.rs:182`) then `servers.shutdown_all()`; single-shot
  exits after one call (`main.rs:104`).
- `tddy-tools` routes `restructure` to the daemon when `TDDY_INDEX_SOCKET` is set
  (`index_client.rs:33`), otherwise runs `restructure_cli::run` in process.
- Journal records ops by index (`journal.rs`); `--from N` is an index.

### State B (Target)

- `PlanStore` per root inside `WorkspaceIndex`; per invocation inside `restructure_cli::run`.
- `LoadedPlan { key, ops, on_disk_hash, dirty }`; ops looked up by `OpId`.
- After `commit_operation` for op k, `refresh_after_op` translates pending ops' hints/ranges through
  op k's edits and renames, recomputes fingerprints for items op k edited via `resolve_item`, marks
  the plan dirty.
- Flusher: dirty plans written after a short debounce; `flush_all` at run end, unload, shutdown.
- Journal and events carry op ids alongside indices; `--from` accepts either.

### Delta

#### tddy-code-restructuring
- `plan.rs`: `id`, `to_jsonl`; `plan_store.rs` (new); `runner.rs`: execute `(plan, from op id)` from a
  store; `journal.rs`: op id in records; `restructure_args.rs`/`restructure_cli.rs`: subcommands,
  per-invocation store, flush at exit.

#### tddy-index-daemon
- `proto/code_index.proto`, `service.rs`, `queries.rs`/`operations.rs` (store-backed),
  `apply.rs` (`for_plan`, refresh), `index.rs` (store per root), `serve.rs`/`main.rs` (flush on
  shutdown and exit), `cli.rs` (subcommands), `render.rs`.

#### tddy-tools
- `index_client.rs`: route `load`/`unload`/`plans`.

## Implementation Milestones

- [x] Op ids assigned, refused on duplicate, flushed
- [x] Store load/unload/list; Apply from the store; implicit load
- [x] Per-op refresh of the applied plan (v1 ranges through the ledger; item anchors re-relativized)
- [x] Flush policy + clobber refusal + shutdown flush
- [x] RPCs and CLI on both front ends (`tddy-tools` and the `tddy-index-daemon` single-shot line)
- [x] `StatePaths::for_plan` in the daemon (via `runner::open_plan_run`)
- [ ] `--from <id>` over the wire — `ApplyRequest` has no field for it; refused with a `TODO(plan-store)`
      in `tddy-tools/src/index_client.rs` (see Decisions)

## Testing Plan

### Testing Strategy

The store is unit-tested in `plan_store.rs` over temp directories (no LSP). The run-by-reference and
refresh behaviour is acceptance-tested in `tddy-code-restructuring/tests/` against rust-analyzer on
harness fixtures. The RPCs, shutdown flush and code-issue fix are acceptance-tested in
`tddy-index-daemon/tests/code_index_service_acceptance.rs` in process and, for `SIGTERM`, in
`detached_daemon_production.rs`.

## Acceptance Tests

### tddy-code-restructuring — `tests/plan_store_acceptance.rs`

- `loading_a_plan_without_ids_assigns_them_and_the_flush_writes_them`
- `two_ops_sharing_an_id_are_refused_as_malformed`
- `a_flush_onto_a_plan_changed_on_disk_is_refused_and_leaves_the_file`
- `apply_executes_the_loaded_ops_not_the_edited_file` (live rust-analyzer)
- `apply_without_a_daemon_still_flushes_the_plan_at_exit` (live rust-analyzer)

### tddy-code-restructuring — `tests/plan_store_resume_acceptance.rs` (added in green)

- `the_plan_written_back_after_its_first_operation_anchors_the_second_below_where_it_was` (live
  rust-analyzer)
- `a_resumed_run_applies_the_second_operation_where_its_text_now_is` (live rust-analyzer)

Not in the red phase: the changeset's `the_applied_plans_pending_op_follows_an_edit_inside_its_item`
was never written — the stub-resolver unit test below pins the item-anchor refresh instead.

### tddy-code-restructuring — `src/plan_store.rs` (unit, no server — a stub `ItemResolver`)

The per-op refresh is pinned here rather than through a real extraction, whose inserted line count
is rust-analyzer's to choose and not something a test can state:

- `a_pending_range_anchor_moves_down_past_lines_an_applied_op_inserted_above_it`
- `a_pending_item_anchors_hint_and_fingerprint_follow_an_edit_to_its_item`
- `a_pending_anchor_follows_a_file_the_applied_op_moved`
- `flush_dirty_leaves_a_plan_dirty_for_less_than_the_debounce`
- `loading_a_plan_already_held_keeps_the_held_copy`
- `plan.rs`: `a_plan_written_back_reads_as_the_same_plan`

### tddy-index-daemon — `tests/code_index_service_acceptance.rs`

- `apply_of_an_unloaded_plan_loads_it_and_list_plans_shows_it`
- `unload_all_flushes_and_drops_every_plan_of_the_root`
- `a_second_plan_applies_after_a_first_ran_under_the_same_root` — the code issue, reproduced:
  today it fails with `a journal already exists for this plan — pass --resume to continue it`
- `a_dirty_plan_reaches_disk_within_the_flush_interval`

### tddy-index-daemon — `tests/dual_transport_acceptance.rs`

- `sigterm_flushes_every_dirty_plan_before_exit` — the real binary, served over gRPC, sent
  `SIGTERM` (in this suite rather than `detached_daemon_production.rs`, whose tests are all
  `#[ignore]`d behind `nix develop`)

### tddy-tools — `tests/restructure_cli_acceptance.rs`

- `restructure_load_without_a_daemon_is_refused_as_needing_one`

All fail at this node's own `TODO(plan-store)` stubs (or, for the code-issue test, at today's
repo-scoped journal).

## Technical Debt & Production Readiness

- Every `TODO(plan-store)` stub of the contract commit is implemented. One new marker remains:
  `tddy-tools/src/index_client.rs` (`from_index`) — `--from <id>` through the daemon.
- The crash window between a committed operation and its plan write-back is detected, not closed:
  the next resume refuses (`PlanOutOfSync`) and the remainder must run from a new plan file. There
  is no repair — the lost refresh is not reconstructed. A hand edit of a pending *anchor* between
  runs is refused the same way; edits to anything else (`name`, `variant`) are not.
- A journal from before write-back, resumed over a plan of ranges and symbols, runs as it did: its
  operations stay unnumbered and the plan is neither refreshed nor written back (mixing epochs in
  one plan would leave anchors that match neither). Over an item-anchored plan it is refused.
- New error variants: `PlanChangedOnDisk`, `NeedsIndexDaemon`, `PlanOutOfSync`, `PlanUnverifiable` (all `FailedPrecondition`); `ItemAnchorsOnContinuedRun` removed.
- `RefactorOp.id` added; 14 struct literals across the crate and its tests gained `id: None`.

## Decisions & Trade-offs

- **Anchors in the store are current, so a run translates through its own edits only.** After each
  committed operation the pending operations are rewritten for the tree and the plan is flushed
  synchronously (not left to the debounce), so the file never lags the journal by more than that one
  step. `runner::open_plan_run` still verifies the ledger checkpoint against the journal but starts
  the run's translation ledger empty: folding the journal would translate anchors through earlier
  runs' edits a second time. Proven end to end by `plan_store_resume_acceptance`.
- **Resume is verified by a journalled digest of the pending anchors, written between the operation
  and the plan.** Order per operation: journal `completed` → refresh in memory → journal
  `plan_synced` (digest of ops after this one: ids, anchors, `also`) → atomic plan write. A resume
  recomputes the digest from the plan it holds and compares it with the last completed operation's
  record. Every crash point refuses and none redoes or skips an operation: before the record there
  is no digest (refused), between record and write the file does not match it (refused), after the
  write it matches. "Written by this version" is read off the journal itself — such records carry
  `op_id` — so no header changed. Item anchors are resolved only for the operations a run executes.
- **A refresh leaves an item it can no longer resolve as written.** `MalformedPlan` from the
  resolver (item absent, ambiguous, outside the file) is the stale-op case the boundaries put out of
  scope; any other failure (cancelled, server defect) propagates.
- **`unload` drops a plan whose file changed on disk without writing it**, since "unload it and load
  it again" is how a refused flush tells the human to take their edit over the store's. Every other
  write failure keeps the plan held.
- **`load` is all or nothing**; naming a plan to `unload` that is not held is refused before anything
  is dropped.
- **`--from <id>` reaches runs with no daemon only.** `ApplyRequest.from_op` would break every
  exhaustive `ApplyRequest` literal in the daemon's acceptance suite, which this change does not
  edit. Journal records and `OperationApplied` events carry the op id (`op_id`).
- **A dry run writes nothing**: no refresh, no flush, ids stay in memory. Neither does a run refused
  before its first operation — its refusals say "nothing was written", so the plan file is left
  without the ids a load would have given it.

- **Store in the library, not the daemon** — one code path for served and one-shot runs, per the
  daemon crate's own "two lifetimes, one implementation" rule.
- **Implicit load on `Apply`** — developer's choice; `load` exists for batch loading and `unload`
  for release.
- **Refuse, don't clobber** a plan edited on disk while loaded — the file is the human's source.
- **Op ids are opaque strings** assigned on load, so reordering or inserting ops by hand is safe.

## Refactoring Needed

### From @validate-changes (Change Validation)
- DONE `packages/tddy-code-restructuring/docs/item-anchors.md` was edited directly in `4cc9d220`
  (CLAUDE.md: `packages/*/docs/` changes go through the changeset). Reverted to the merge base; the
  wrap carries the delta: replace the `ItemAnchorsOnContinuedRun` table row with `PlanOutOfSync { op }`
  (a continued run whose plan does not match what the journal recorded) and `PlanUnverifiable
  { applied }` (a journal from before write-back over an item-anchored plan), and replace the "A
  continued run refuses item anchors" paragraph with: a continued run resolves item anchors against
  the tree it continues on, over the operations it will execute, after checking the plan against the
  journal's `plan_synced` digest.
- DONE `ledger.rs:136` named the removed `runner::resolve_item_anchors`; now `item_anchor::resolve_item_anchors`.
- OPEN, needs the developer: the `legacy` path (a journal from before write-back resumes a range/symbol
  plan as it always did, unnumbered and not written back) is a compatibility branch; CLAUDE.md asks
  consent before fallbacks. Recorded under Technical Debt; consent not confirmed.

### From @validate-tests (Test Quality)
- DONE `sigterm_flushes_every_dirty_plan_before_exit`: `serving.wait()` was unbounded and the load result
  was asserted before the process was reaped; now a bounded wait that kills a survivor, results asserted after.
- DONE `a_second_plan_applies_after_a_first_ran_under_the_same_root`: the first run's result is discarded,
  so the test could pass without ever having written run state; now asserts `.restructure/` exists first.
- DONE added `an_operation_is_found_by_its_id_whatever_its_place_in_the_plan` (`PlanStore::op` had no
  caller or test) and `a_pending_item_anchor_the_edit_left_unresolvable_keeps_its_anchor_as_written`
  (the stale-item decision under Decisions had no test).
- OPEN (INFO): `a_plan_file` and the `EXTRACT_*` constants are duplicated across `plan_store_acceptance.rs`
  and `plan_store_resume_acceptance.rs`; moving them into `tests/harness/mod.rs` was left because #539 and
  #540 both edit that file.

### From @prod-ready (Production Readiness)
- No mock or dev fallback, no `println!`/`eprintln!` added to any `src/`. Lock `.expect("a root's plan
  store")` on std mutexes means a panic while holding a store poisons it for the root (INFO).
- `TODO(plan-store)` in `tddy-tools/src/index_client.rs` (`from_index`): `--from <id>` through the daemon.
  Deliberately deferred (`ApplyRequest.from_op` would break every exhaustive `ApplyRequest` literal in
  the daemon's acceptance suite); refused with an error naming the workaround. Owner: a follow-up that
  edits that suite. No other marker remains.
- `PlanStore::op` has no production caller yet; kept as the contract's lookup by `(plan, op id)`.

### From @analyze-clean-code (Code Quality)
- Must refactor, not done here: `plan_store.rs::refreshed` (76 lines, nesting 5; #539 extends this file's
  refresh, so it is not split under it), `runner/entry_points.rs::apply_held_plan` (151 lines, the
  pre-existing apply loop) and `tddy-index-daemon/src/apply.rs::apply_held_plan` (136, same loop),
  `record_applied_op` (7 parameters) and `commit_operation` (7).
- File length: see Validation Results.

## Validation Results

**2026-10-02**

- validate-changes (twice): stack gate clean (`origin/master..HEAD` is this PR's six commits); parent
  surfaces (anchor kinds, resolver, v2 header) untouched; the one consented change is the removal of
  `ItemAnchorsOnContinuedRun`/`runner::resolve_item_anchors`. No critical findings after the
  `item-anchors.md` revert above.
- validate-tests: the tests this PR added or changed were analysed; 0 critical; see Refactoring Needed.
- validate-prod-ready: clean apart from the recorded `TODO(plan-store)`.
- analyze-clean-code: overall C (3+ must-refactor items, all pre-existing loop bodies or in a file #539 owns).
- File-length gate (production lines before the first `#[cfg(test)]`, merge base to HEAD):

| File | Before | After | Record | Also in #539 / #540 |
|---|---|---|---|---|
| `tddy-code-restructuring/src/plan.rs` | 799 | 887 | `oversized-file-plan.md` (unclaimed) | no |
| `tddy-code-restructuring/src/runner/entry_points.rs` | 602 | 814 | `oversized-file-runner-entry-points.md` (unclaimed) | no |
| `tddy-code-restructuring/src/plan_store.rs` | new | 522 | none | #539 |
| `tddy-code-restructuring/src/crate_move/cluster.rs` | 611 | 611 | n/a (one `id: None` literal in tests) | #540 |

## TODO

- [x] Record initial discovery (`2026-09-26-index-daemon-plan-store-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress (this changeset, the code-restructuring skill, module docs;
      `packages/*/docs/` is left to the wrap)
- [ ] Repeat Red→Green→Update cycle until feature complete
- [x] Run scoped tests (`./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools`); CI for the rest
- [x] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [x] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [x] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [x] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [x] Final validation (/validate-changes)
- [x] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`) — scoped to the three packages
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-26-index-daemon-plan-store-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
