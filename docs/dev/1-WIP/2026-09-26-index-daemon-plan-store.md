# Changeset: Index daemon plan store — plans are loaded once, executed by reference, and flushed back

**Date**: 2026-09-26
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-26-index-daemon-plan-store-initial-discovery.md).

## Stack

`#live-plan` 2/7 — branch `feature/live-plan/plan-store`, base `feature/live-plan/item-anchors`.
PR: _recorded in wave 2_

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

- [ ] Op ids assigned, refused on duplicate, flushed
- [ ] Store load/unload/list; Apply from the store; implicit load
- [ ] Per-op refresh of the applied plan (v1 ranges through the ledger; item anchors re-relativized)
- [ ] Flush policy + clobber refusal + shutdown flush
- [ ] RPCs and CLI on both front ends
- [ ] `StatePaths::for_plan` in the daemon

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
- `apply_executes_the_loaded_ops_not_the_edited_file`
- `a_resume_from_op_two_applies_the_same_edit_as_a_clean_run`
- `the_applied_plans_pending_op_follows_an_edit_inside_its_item`
- `a_flush_onto_a_plan_changed_on_disk_is_refused_and_leaves_the_file`
- `apply_without_a_daemon_still_flushes_the_plan_at_exit`

### tddy-index-daemon — `tests/code_index_service_acceptance.rs`

- `apply_of_an_unloaded_plan_loads_it_and_list_plans_shows_it`
- `unload_all_flushes_and_drops_every_plan_of_the_root`
- `a_second_plan_applies_after_a_first_completed_under_the_same_root`
- `a_dirty_plan_reaches_disk_within_the_flush_interval`

### tddy-index-daemon — `tests/detached_daemon_production.rs`

- `sigterm_flushes_every_dirty_plan_before_exit`

### tddy-tools — `tests/restructure_cli_acceptance.rs`

- `restructure_load_without_a_daemon_is_refused_as_needing_one`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Store in the library, not the daemon** — one code path for served and one-shot runs, per the
  daemon crate's own "two lifetimes, one implementation" rule.
- **Implicit load on `Apply`** — developer's choice; `load` exists for batch loading and `unload`
  for release.
- **Refuse, don't clobber** a plan edited on disk while loaded — the file is the human's source.
- **Op ids are opaque strings** assigned on load, so reordering or inserting ops by hand is safe.

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## TODO

- [x] Record initial discovery (`2026-09-26-index-daemon-plan-store-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests (`./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools`); CI for the rest
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-26-index-daemon-plan-store-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
