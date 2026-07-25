# Lazy, semaphore-bounded worktree disk usage (streamed)

**Components:** `tddy-daemon::worktrees` (new `WorktreeSizeCalculator`), `ConnectionService` (new `StreamWorktreeStats` + `CalculateWorktreeSize`), web `WorktreesScreen` / `WorktreesAppPage` / `SessionWorktreeTab`
**Updated:** 2026-07-25
**Status:** Draft (Plan-Red)
**Supersedes the disk-size half of:** [worktrees.md](worktrees.md), [session-worktree-inspector.md](session-worktree-inspector.md)

## Overview

Today a worktree's on-disk size is computed **eagerly and in a project-wide batch**: `ListWorktreesForProject(refresh: true)` walks *every* worktree's directory synchronously (a hand-rolled `fs::read_dir` traversal), plus `git diff --numstat`, and writes one `worktree_stats.json`. On a machine with many large worktrees this is slow, unbounded (N parallel walks if several clients refresh at once), and all-or-nothing — the caller waits for the whole project before seeing anything.

This feature makes the **directory-size walk** (the expensive part) **lazy, per-worktree, and centrally rate-limited**, with an explicit lifecycle the UI can render:

- Each worktree tracks its size independently with a status of **`None`** (never calculated), **`Calculating`** (a walk is in flight), or **`Cached`** (a value exists), plus the **timestamp of the last calculation**.
- A **daemon-global semaphore** caps how many size walks run at once (**default 2**). Additional requests queue behind it rather than piling on the disk.
- Computation is **lazy**: it is triggered on demand — the Session Inspector's Worktree tab triggers a calculation for its one worktree when it is opened and that worktree is `None`; the Worktrees screen can retrigger all of a project's worktrees or trigger an individual one.
- The Worktrees screen renders the whole project and **streams** the results: a first server-streaming frame carries the current snapshot of every worktree (with its status), then one incremental frame per worktree as it flips `Calculating → Cached` carrying the final byte count. The number appears when the walk completes (no live partial-byte animation).

The `git diff` summary (changed files, ±lines) is **unchanged** — it stays cheap and eager; only the directory-size walk is lazy, semaphore-gated, and status-tracked.

## Requirements

### Daemon library — `WorktreeSizeCalculator`

A new component in `tddy-daemon::worktrees` owns the size lifecycle. It is created once and shared (`Arc`), like `WorktreeStatsCache`.

- **Status model.** For a `(project_id, worktree_path)` it reports one of `None` / `Calculating` / `Cached`. `Cached` carries the last computed `disk_bytes` and the `calculated_at_unix_ms` timestamp. `None` is the state for a worktree with no persisted size and no in-flight walk. `Calculating` is a purely in-memory, transient state.
- **Central semaphore.** A single **daemon-global** `tokio::sync::Semaphore` (default **2** permits, injectable for tests) bounds the number of concurrent size walks **across all projects and worktrees**. A calculation acquires a permit before walking and releases it after. The 3rd+ concurrent request waits for a permit — it does not spawn a parallel walk.
- **Lazy enqueue.** `enqueue(project_id, path)` starts a calculation: it immediately marks the worktree `Calculating` and publishes that transition, then (once a permit is free) runs the directory-size walk on a blocking thread, and on completion marks it `Cached` with the new bytes + timestamp, publishes that, and persists it. Enqueuing a worktree already `Calculating` is a no-op (de-duplicated) — a second visitor does not start a second walk.
- **Subscription.** `subscribe(project_id)` returns a stream of per-worktree updates (path + new status + optional bytes/timestamp) for that project, so the RPC layer can forward increments to connected clients. `snapshot(project_id)` returns the current state of every known worktree for the stream's first frame.
- **Persistence.** A `Cached` size and its timestamp survive a daemon restart (persisted under the existing per-project stats cache root); after restart the worktree reads back as `Cached`, not `None`, and is served without re-walking.
- **Injectable sizer.** The directory-size function is injectable so tests can substitute a deterministic, instrumented walker (to observe concurrency and gate completion); production uses the existing `directory_size_bytes_best_effort`.

### RPC surface (`ConnectionService`)

- **New server-streaming `StreamWorktreeStats(StreamWorktreeStatsRequest) returns (stream WorktreeStatsEvent)`.** Authenticates `session_token`. Emits a first `WorktreeStatsEvent` carrying the full snapshot of the project's worktrees (each with `size_status` + `size_calculated_at_unix_ms` + cheap eager diff/branch), then one event per worktree as its size flips `Calculating → Cached`. When the request's `recalculate_all` is set, every worktree is (re)enqueued on subscribe; otherwise only worktrees currently `None` are lazily enqueued (opening the feed calculates what has never been calculated). The stream stays open until the client unsubscribes (mirrors `StreamHostStats`).
- **New unary `CalculateWorktreeSize(CalculateWorktreeSizeRequest) returns (CalculateWorktreeSizeResponse)`.** Enqueues a (re)calculation for one worktree (membership-gated by `git worktree list`, like `RemoveWorktree`); the result surfaces on any open `StreamWorktreeStats`. Used by the Worktrees screen's per-row control and by the Session Inspector's Refresh.
- **`WorktreeRow` gains** `WorktreeSizeStatus size_status` and `int64 size_calculated_at_unix_ms`. `WorktreeSizeStatus` is `{ UNSPECIFIED, NONE, CALCULATING, CACHED }`.
- **`ListWorktreesForProject` stays** (unary cached read) for non-streaming callers; it now reports the size status from the calculator. It no longer performs the eager size walk on `refresh: true` — size is the calculator's job. **Non-breaking:** existing callers keep working.

### Web

- **Worktrees screen (`WorktreesScreen` / `WorktreesAppPage`).** Adds a **Status** column showing `None` / `Calculating` / `Cached` and, for `Cached`, a relative "last calculated" label. Adds a **Recalculate all** control (opens/re-subscribes `StreamWorktreeStats` with `recalculate_all`) and a per-row **Calculate / Recalculate** control (`CalculateWorktreeSize`). Rows update live from the stream: a worktree shows `Calculating` (no size yet) then its size once the increment arrives. A new `useWorktreeStatsStream` hook owns the subscription (mirrors `useHostStats`).
- **Session Inspector → Worktree tab (`SessionWorktreeTab`).** Replaces the 10-minute `ListWorktreesForProject` poll with the stream filtered to the session's own worktree. On open, if the worktree is `None`, the tab is calculated lazily (via the stream's default enqueue) and shows `Calculating` → size. **Refresh** calls `CalculateWorktreeSize` for that one worktree.

## Acceptance criteria

### Daemon library (`WorktreeSizeCalculator`) — `tddy-daemon`

- [ ] A worktree with no persisted size and no walk in flight reports status `None`.
- [ ] Enqueuing a size calculation transitions the worktree `None → Calculating → Cached`, publishing each transition; the final `Cached` state carries the walked byte count and a recorded `calculated_at_unix_ms`.
- [ ] The central semaphore bounds concurrent size walks to **2**: with four worktrees enqueued against a gated sizer, no more than two walks run at once; the remaining two start only as permits free.
- [ ] A `Cached` size + timestamp is persisted and served after reload without re-walking (the sizer is not invoked again).
- [ ] Recalculating a single worktree re-walks only that worktree and leaves the others' cached values untouched.

### Web — Worktrees screen (Cypress component)

- [ ] Each worktree row shows its size status; a `None` worktree shows no byte size and a "Calculate" control.
- [ ] A `Calculating` worktree shows a calculating indicator and no byte size yet.
- [ ] A `Cached` worktree shows its formatted size and a "last calculated" label.
- [ ] "Recalculate all" invokes the project-wide recalculation.
- [ ] A row's "Calculate" invokes calculation for that single worktree's path only.

## Testing plan

- **Level:** Rust integration (daemon library) + Cypress component (web).
- **Rust:** `packages/tddy-daemon/tests/worktree_size_calculator_acceptance.rs` — status model, `None→Calculating→Cached` transitions with recorded timestamp, semaphore-bounded concurrency (=2) via a gated injectable sizer, persistence-without-recompute, single-worktree recalculation isolation. Run: `cargo test -p tddy-daemon --test worktree_size_calculator_acceptance`.
- **Web:** `packages/tddy-web/cypress/component/WorktreesScreenDiskUsage.cy.tsx` — status rendering (None/Calculating/Cached + last-calculated label), Recalculate all, per-row Calculate. Run: `bun run cypress:component`.
- **Deferred to `/red` (needs proto regen):** wire-level `StreamWorktreeStats` daemon RPC test (snapshot-first + per-worktree increments, `recalculate_all`) and the `useWorktreeStatsStream` streaming-hook test — both depend on the regenerated `connection_pb.ts` / prost types and land with the green proto change.

## Scope

- **In scope:** per-worktree lazy size lifecycle; daemon-global semaphore (default 2); `None/Calculating/Cached` + last-calculated time; `StreamWorktreeStats` (snapshot + increments) and `CalculateWorktreeSize`; Worktrees screen status column + recalculate controls; Session Inspector Worktree tab on the stream.
- **Out of scope:** live partial-byte progress during a walk (final value only); making `git diff` lazy (stays eager); per-project (rather than daemon-global) semaphores; routing worktree RPCs to remote hosts (local daemon only, unchanged); removing `ListWorktreesForProject`.

## Related documentation

- [Web Worktrees manager](worktrees.md) — project-wide list, `RemoveWorktree`, `WorktreeStatsCache`.
- [Session Worktree inspector](session-worktree-inspector.md) — per-session Worktree tab (clear/delete/restore unchanged).
- [Streamed host stats](host-stats-footer.md) — `StreamHostStats`, the server-streaming precedent this feature follows.
