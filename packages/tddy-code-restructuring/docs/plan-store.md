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
being applied, and no other plan:

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
its text now is) run against a real rust-analyzer. The refresh is pinned in `plan_store.rs` unit tests
over a stub `ItemResolver`, because the lines an extraction inserts are rust-analyzer's to choose. The
daemon's RPCs, the periodic flush and the plan-scoped run state are in
`tddy-index-daemon/tests/code_index_service_acceptance.rs`; `SIGTERM` flushing is in
`dual_transport_acceptance.rs`.
