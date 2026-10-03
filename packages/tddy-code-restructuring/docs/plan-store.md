# Plan store

How a plan is held in memory, executed by reference and written back. The product contract is
[Rust code restructuring](../../../docs/ft/coder/rust-code-restructuring.md#plan-store); the authoring
view is the skill's `references/plan-schema.md`.

## One type, two lifetimes

`plan_store::PlanStore` holds the plans of one workspace root. `tddy-index-daemon` keeps one per root
across requests; `restructure_cli::run` keeps one for the length of an invocation. There is one code
path, so a served run and a one-shot run cannot disagree about what a plan means.

| Item | Role |
|---|---|
| `PlanKey` | A plan's identity: its path relative to the root (absolute when it lies outside it) |
| `LoadedPlan` | The parsed `Plan`, whether it is dirty, and the hash of the file when the store last read or wrote it |
| `FlushPolicy { debounce }` | How long a plan may stay dirty before `flush_dirty` writes it (the daemon uses one second) |
| `PlanStore::{load, unload, unload_all, list, get, op}` | The registry; `op` looks an operation up by `(plan, op id)` |
| `PlanStore::refresh_after_op` | Rewrites the applied plan's pending operations for the tree the operation left |
| `PlanStore::{fold_foreign_op, reresolve_files, stale_ops}` | Keep every *other* loaded plan current, and say which of its operations are stale; see [Live plans](#live-plans) |
| `PlanStore::{flush_dirty, flush, flush_all}` | The write-back |
| `pending_digest` | A digest of the operations after the one just applied: ids, anchors, `also` |

## Operation ids

`RefactorOp.id` is an optional `OpId`. `Plan::assign_missing_op_ids` gives `op-<n>` to every operation
without one, numbering past the highest id the plan already uses, so an operation inserted by hand
cannot collide. Duplicate ids are `MalformedPlan` at load. `Plan::to_jsonl` writes a plan back in the
executor's key order with defaults omitted, and a plan written back parses as the same plan. The
journal and the `OperationApplied` event carry `op_id` beside the index; `--from` takes either.

## The refresh

After each committed operation `refresh_after_op` translates the *pending* operations of the plan
being applied; the other loaded plans are carried through the same edit by [the fold](#live-plans):

| Pending anchor | Becomes |
|---|---|
| `range` | moved by the lines the applied operation's edits inserted or removed above it, through the ledger |
| `item` / `items` | hint and relative range translated; the fingerprint of an item the operation edited recomputed through `resolve_item` on the post-operation tree |
| any `file` | the new path, when the operation moved the file |

A pending item the resolver can no longer find (`MalformedPlan`: absent, ambiguous, outside the file)
keeps its anchor as written, for the next run to refuse; any other resolver failure propagates.

`runner::open_plan_run` verifies the ledger checkpoint against the journal but starts the run's
translation ledger empty. Folding the journal would translate anchors that already describe the
edited tree through earlier runs' edits a second time.

## Live plans

Two things move a held plan's anchors besides its own operations: an operation of **another** held
plan, and a change nobody here made. Both rewrite what is safe to rewrite and mark the rest **stale**.
Neither re-targets a stale operation: an edit that overlaps an anchored range could have removed what
the range names, so the author re-anchors it.

| Module | Holds |
|---|---|
| `plan_store.rs` | `PlanStore::{fold_foreign_op, reresolve_files, stale_ops}` (delegations), `StaleReason`, `OpStaleness` |
| `plan_store/live.rs` | `Liveness` (the stale reasons and the files the store's own runs wrote), re-resolution, `stale_ops`, the per-file hint refresh |
| `plan_store/live/fold.rs` | The foreign-op fold |
| `plan_store/refresh.rs` | `refresh_after_op`, `refresh_pending`, and `rebase_plan_file` (the `snapshot` re-resolution) |

**`fold_foreign_op(from, op, edit, resolver)`** judges every operation of every plan except `from`,
line by line, against the edit the operation produced, in the coordinates of the text it was made
against. A range anchor the edit only moves is translated through the edit's line deltas; an item
anchor's hint and relative range follow, and its fingerprint is recomputed where the edit touched an
item outside the anchored range. A `file` hint follows a rename or move. An edit that **overlaps** an
anchored range marks the operation `StaleReason::EditedBy { plan, op }` and leaves it as written.
Nothing changes unless every plan could be folded.

**`reresolve_files(files, resolver)`** re-resolves every held item anchor in `files`. An intact item
rewrites its hint and the plan's per-file hint (`sha256`, `modified`); an item whose text changed is
`ItemChanged`; one the file no longer declares is `ItemNotFound { file }`. Re-resolution clears `ItemChanged` / `ItemNotFound` once the tree says
the item is intact again, but never `EditedBy`. Files the store's own runs wrote since the last call are
skipped: the run already carried every plan through those edits.

**Staleness is derived and held in memory** beside the plans in `Liveness`, dropped when a plan is
unloaded, never serialised: the plan on disk stays a plan, and a stale operation's anchor keeps its
pre-edit coordinates. An unloaded plan is not reached by either entry point.

**Who calls them.**

| Caller | What it does |
|---|---|
| `runner::record_applied_op` | After each committed operation, settles the plan that ran (refresh, journal digest, flush) and only then folds the operation into every other held plan and writes those back, so a failure folding another plan never leaves the plan that ran with an edit on disk and no digest. The daemon's apply loop and a one-shot `apply` both call it |
| `tddy-index-daemon/src/plan_upkeep.rs` | For the files the tree comparison reports changed that the daemon did not write, calls `reresolve_files` |
| `runner::refuse_a_stale_pending_op` | Before any read or write of an apply, in both apply loops: `RestructureError::StaleOperation` (`FailedPrecondition`) for the first stale operation at or after the run's start, up to `--stop-after`; a dry run is not refused |
| `runner::stale_findings` | One `check` finding per stale operation |
| `runner::snapshot_resolving` | `restructure snapshot` of an item-anchored plan, through `rebase_plan_file` |
| `console::stale_operations` | The one renderer for stale operations: the in-process CLI, the daemon and `tddy-tools` all call it |

**Not covered.** A file the daemon sees deleted is not re-resolved; the store holds no record of which
operations a plan has already run, so a foreign edit overlapping an already-applied operation's old
anchor marks it stale (`apply` ignores it, `ListPlans` shows it); the applied plan's own v2 file hints
are not rewritten by `refresh_after_op`; and `restructure snapshot` of an item-anchored plan through
`tddy-tools` starts its own language server, there being no `Snapshot` RPC.

## Order per operation, and what a crash leaves

1. journal `completed` for operation *k*;
2. refresh the plan in memory;
3. journal `plan_synced`: the digest of operations after *k*;
4. write the plan atomically (temporary file, rename).

`runner/resume.rs` recomputes the digest from the plan a continued run holds and compares it with the
last completed operation's `plan_synced`. A crash before step 3 leaves no digest, between 3 and 4 leaves
a file that does not match it, and after 4 leaves a match; the first two are refused as `PlanOutOfSync
{ op }`. None redoes or skips an operation. "Written by this version" is read off the journal itself,
since such records carry `op_id`. A journal from before write-back resumes a plan of ranges and symbols
unnumbered, neither refreshed nor written back (mixing epochs in one plan would leave anchors that
match neither), and refuses an item-anchored plan as `PlanUnverifiable { applied }`.

There is no repair for the crash window; the remainder runs from a new plan file.

## Flush and the file on disk

A flush writes a temporary file beside the plan and renames it. Before writing it compares the file's
current hash with `on_disk`; a mismatch is `PlanChangedOnDisk`, and the file is left alone. `unload`
drops such a plan without writing it, because "unload and load again" is the remedy that error gives.
Every other write failure keeps the plan held. A dry run writes nothing, and neither does a run refused
before its first operation: its refusals say nothing was written, so the file gets no ids a load would
have given it. After a committed operation the run flushes synchronously rather than waiting for the
debounce.

## In the daemon

`WorkspaceIndex` holds a `PlanStore` per root, outliving the root's language server: a reaped server
leaves the plans, perhaps dirty, in memory. A background tick (250 ms) calls `flush_dirty`; `serve.rs`
and `main.rs` call `flush_all` on `^C`/`SIGTERM` and before a single-shot call returns. The apply
loop copies the plan out of the store rather than holding the store through a baseline compile check
that takes minutes. A lock held while a store operation panics poisons that root's store.

## Errors

| Variant | Status | When |
|---|---|---|
| `PlanChangedOnDisk { plan }` | `FailedPrecondition` | a flush onto a file changed since it was loaded |
| `NeedsIndexDaemon { command }` | `FailedPrecondition` | `load`, `unload` or `plans` with no daemon |
| `PlanOutOfSync { op }` | `FailedPrecondition` | a continued run's plan does not match the journal's digest |
| `PlanUnverifiable { applied }` | `FailedPrecondition` | an item-anchored plan over a journal from before write-back |

## Testing

`tests/plan_store_acceptance.rs` (ids assigned and flushed, duplicates refused, clobber refused, apply
from the loaded ops, a no-daemon run still flushing) and `tests/plan_store_resume_acceptance.rs` (a
two-operation plan written back after its first operation, and a resumed run applying the second where
its text now is) run against a real rust-analyzer. The refresh and the live-plan fold and re-resolution are pinned in `plan_store.rs` unit tests
over a stub `ItemResolver`, because the lines an extraction inserts are rust-analyzer's to choose. The
daemon's RPCs, the periodic flush and the plan-scoped run state are in
`tddy-index-daemon/tests/code_index_service_acceptance.rs`; `SIGTERM` flushing is in
`dual_transport_acceptance.rs`. `tests/live_plans_acceptance.rs` runs two plans in one store against a
real rust-analyzer (the fold keeping a second plan's anchor on its item, an edit inside it marking the
operation stale, the refusal before any write, hand edits above and inside an item, `snapshot`
re-resolution); the daemon's `tests/live_plans_acceptance.rs` covers the apply loop folding a test
binary move into another plan's file hint and an unloaded plan staying byte-identical.
