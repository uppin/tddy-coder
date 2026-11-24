# Changeset: work-queue-live-view

**Type:** Fix
**Date:** 2026-11-24
**Branch:** `fix/workqueue-view-live-refresh`

## Problem

Observed 2026-11-24: an orchestrator added four items and two session links to its queue through MCP
tools (`work_item_add`, `work_item_link_session`) while the operator had the queue's Work Queue tab
open. The tab stayed empty until the operator left it and came back.

- `WorkQueueView` read the queue once (`GetWorkQueue`) when it opened and afterwards changed it only
  from its own mutations' answers. Nothing told it about a change made anywhere else.
- `.catch(() => undefined)` on that read meant a failed first load looked exactly like an empty queue.
- The daemon had no general "queue changed" signal. The `work_queue` watch event covers only moves,
  lands, propagations, refusals and fired schedules, and wakes only the owner's agent.
  `StreamWorkQueueStatus` re-read only on GitHub inbox events, so a newly added item had no status row.
- The `workQueue.ts` header still described its input as `SessionEntry.workQueueJson` only.

## State B

- **`WorkQueue.revision`** (`tddy-changeset`): 1 at creation. `SessionWorkQueues::update` compares the
  queue before and after the closure, leaving the revision out of the comparison. On a real change it
  raises the revision by one and writes the record. On no change it writes nothing and keeps the
  revision. A closure cannot set its own revision. The revision is serialized into every
  `work_queue_json` (skipped when 0), so every mutation answer carries it with no proto change.
  `SessionWorkQueues::record_path` and `WorkQueueStore::record_path` name the file.
- **`WatchWorkQueue`** (`work_queue.proto`): `WatchWorkQueueRequest { session_token, queue }` →
  `stream WorkQueueSnapshot { revision, work_queue_json, summary }`.
  - The current snapshot comes first, then one per new revision.
  - Writes close together coalesce into one snapshot of the latest.
  - A queue removed while watched ends the stream with `not_found`.
  - An unknown queue is `not_found` at open.
  - A peer's address is `failed_precondition`; the stream is not forwarded.
- **Fan-out hook** (`tddy-daemon-rpc/src/work_queue/queue_changes.rs`, `QueueChanges`): a `notify` v7
  watch on the record's session directory, filtered to the record's name. Every write in any process
  ends in an atomic rename of that file. The watch is started before the snapshot is read. A watcher
  error counts as "read again". Readers send only when the revision has risen.
  `StreamWorkQueueStatus` wakes on it too, so an added item gets its status row at once. For a peer's
  address it now answers `failed_precondition`, as the watch does.
- **Web** (`tddy-workqueue-web`):
  - `useWorkQueueWatch` holds the newest copy. `queueRevision.supersedes`: the first copy is taken, a
    newer one replaces it, an equal or older one is ignored, and an unnumbered one is taken as it comes.
  - Both stream snapshots and mutation answers go through `apply`.
  - Reconnect backoff is 1 s, 2 s, 4 s … up to 30 s, and resets after a snapshot.
  - Terminal codes: `not_found`, `unauthenticated`, `permission_denied`, `invalid_argument`,
    `failed_precondition`, `unimplemented`.
  - `WorkQueueView` requires `watchClient: Client | null` (no default to `workQueueClient`) and opens
    both the watch and `StreamWorkQueueStatus` on it.
  - Display states:
    - `work-queue-load-error` replaces the list when nothing has loaded.
    - `work-queue-live-notice` reads "Reconnecting…" or "Live updates stopped: …" above a loaded queue.
    - A `null` client is an ended watch naming the unreachable host.
- **Callers** (`tddy-session-web`): `SessionWorkQueueTab`, the only place `WorkQueueView` is rendered,
  resolves `watchClient` with `useHostClient(WorkQueueService, session.daemonInstanceId ||
  selectedInstanceId)`.
- **Test infrastructure** (`tddy-connectrpc-testkit`): the test-only `registerServerStreamFallback` /
  `ServerStreamFallback` is renamed **`registerUnaryBackedStream` / `UnaryBackedStream`**, since it is a
  test fake that lets a unary stub serve a stream, not a runtime fallback. Every Cypress support file
  that registers it is updated (`tddy-web`, `tddy-session-web`, `tddy-session-create-web`,
  `tddy-workqueue-web`), and so is `tddy-web`'s spec, now `UnaryBackedStream.cy.tsx`. It gains two
  options:
  - `holdOpen` keeps the stream open after its one event until the client cancels it, as a watch does.
  - `toRequest` maps the stream's request onto the unary's shape.
  `tddy-workqueue-web` and `tddy-session-web` register `getWorkQueue → watchWorkQueue` with both
  (`queue` → `stack`), so specs that stub only `getWorkQueue` still load the view. **The view itself
  never falls back to `GetWorkQueue`**: a failed or unavailable watch shows the load-error or
  interrupted state.
- `docs/dev/todo/2026-11-22-work-queue-status-stream-is-local-only.md` is **closed**: the web opens the
  status stream on the holding host.

## Packages

| Package | Change |
|---|---|
| `tddy-changeset` | `WorkQueue.revision`; change-only writes; `record_path` |
| `tddy-service` | `WatchWorkQueue`, `WatchWorkQueueRequest`, `WorkQueueSnapshot` |
| `tddy-work-queue` | `ChannelStream<T>` (status and watch streams); `WorkQueueHandler::watch_work_queue` |
| `tddy-daemon-rpc` | `queue_watch.rs`, `queue_changes.rs` (+ `notify = "7"`, operator-approved); status stream wakes on queue changes |
| `tddy-client-web` | regenerated `work_queue_pb.ts` |
| `tddy-connectrpc-testkit` | `registerServerStreamFallback` renamed `registerUnaryBackedStream`; `holdOpen`, `toRequest` |
| `tddy-web`, `tddy-session-create-web` | Cypress support: the renamed registration; `tddy-web` spec renamed `UnaryBackedStream.cy.tsx` |
| `tddy-workqueue-web` | `useWorkQueueWatch`, `queueRevision`, `WorkQueueLiveStatus`, required `watchClient`, status stream on it, `workQueue.ts` doc |
| `tddy-session-web` | `SessionWorkQueueTab` resolves `watchClient` on the holding host |

## Restructuring

`/pr-wrap`'s file-length gate measured `packages/tddy-workqueue-web/src/WorkQueueView.tsx` at **499 → 507**
production lines: this change took it over the 500 budget. The Start-session dialog's pre-fill derivation
moved, unchanged, into `src/startSessionInitialValues.ts` (a pure function of the node, the queue's
nodes, the address, the default branch and remote, and the owner). `WorkQueueView.tsx` is now **484**. The
other files this change touched are under budget (`tddy-changeset/src/changeset/work_queue.rs` is 460 → 469).

## Tests

- `tddy-changeset/tests/session_work_queue_acceptance.rs`:
  - revision 1 → +1 per change
  - a no-op update keeps the revision and does not rewrite the record (same inode, same bytes)
  - a change cannot set its own revision
  - the revision is stored and appears in the JSON
- `tddy-daemon-rpc/tests/work_queue_watch_rpc_acceptance.rs`:
  - the first snapshot is the queue as it stands
  - a write the daemon did not make, a mutation RPC, and an agent's `work_item_add` through
    `ExecuteTool` each reach the watch
  - quick successive writes arrive in rising order and end on the latest
  - a no-op update sends nothing
  - a removed queue ends with `not_found`, and an unknown queue is `not_found`
- `tddy-daemon-rpc/src/work_queue/status_stream_wake_unit_tests.rs`: the status stream sends a new
  snapshot when its queue changes, with no GitHub event behind it.
- `tddy-workqueue-web/src/queueRevision.test.ts` (bun): `revisionOf`, `supersedes`.
- `tddy-workqueue-web/cypress/component/WorkQueueLiveAcceptance.cy.tsx`:
  - an item added outside the view appears
  - an older snapshot after the view's own edit does not undo it
  - a failed load says why instead of showing an empty queue
  - a dropped watch shows "Reconnecting" and is resynced
  - a removed queue ends the watch for good
- `tddy-workqueue-web/cypress/component/WorkQueueWatchUnaryBacked.cy.tsx`: a `getWorkQueue` stub,
  asked with the watched queue as its `stack`, backs the watch, which stays open until cancelled.

## Docs updated at wrap

- `docs/ft/coder/pr-stacking.md`: § *The queue view stays current* (added); § *Vocabulary*, § *Live checks on
  the row* and § *Opening a session's work queue* updated.
- `docs/ft/web/session-drawer.md` § *Planned-PR list* and its RPC list; `docs/ft/web/prstack-package.md`.
- `packages/tddy-workqueue-web/docs/README.md`, `packages/tddy-daemon-rpc/docs/architecture.md`
  (`queue_watch.rs`, `queue_changes.rs`, `status_stream.rs`), `packages/tddy-changeset/docs/architecture.md`
  (revision on change, no-op writes nothing, `record_path`).
- `packages/tddy-session-web/docs/indexing-indicator.md`, `packages/tddy-session-create-web/docs/start-progress.md`:
  the renamed `registerUnaryBackedStream`.

## Prerequisites

- ✅ RESOLVED HERE — `docs/dev/todo/2026-11-22-work-queue-status-stream-is-local-only.md`: the web opens
  `StreamWorkQueueStatus` (and `WatchWorkQueue`) on the host that holds the queue; deleted.

## Wrap

Wrapped 2026-11-24 by `/pr-wrap` on PR #1030. Verification was scoped to the touched packages:
- the `tddy-changeset` suites
- `tddy-daemon-rpc`'s watch acceptance and status-stream wake tests
- the bun suites of `tddy-workqueue-web` and `tddy-session-web`
- the Cypress specs of `tddy-workqueue-web`, `tddy-session-web`, `tddy-session-create-web` and `tddy-web`
  that exercise the view or the renamed testkit helper

Whole-workspace health is CI's.

**Boundary tests:** the `packageBoundary.test.ts` of `tddy-session-web` and `tddy-workqueue-web` now load
the entry point in a `beforeAll` with a 60 s budget. The cold import measured 0.9–5.8 s at load 35–90, and
11.3 s at ~370, against bun's 5 s default. The same pattern stands in seven other packages; per the
operator, those go in a separate PR.
