# Changeset: worktree-disk-usage-streaming

**Type:** Feature
**Status:** WIP (Plan-Red)
**Started:** 2026-07-25
**Packages:** tddy-service, tddy-daemon, tddy-web
**Feature doc:** [docs/ft/web/worktree-disk-usage-streaming.md](../ft/web/worktree-disk-usage-streaming.md)

## Summary

Make per-worktree **disk size** lazy, per-worktree, and centrally rate-limited, with a
`None`/`Calculating`/`Cached` status + last-calculated timestamp the UI can render live. A single
**daemon-global semaphore** (default **2**) bounds concurrent directory-size walks. The Worktrees
screen streams results — a first snapshot frame, then one increment per worktree as it flips
`Calculating → Cached` — via a new server-streaming `StreamWorktreeStats`; individual worktrees can
be (re)triggered with a new unary `CalculateWorktreeSize`. The `git diff` summary stays cheap and
eager; only the size walk changes. Non-breaking: `ListWorktreesForProject` remains for cached reads.

Direct precedent: the `streamed-host-stats` changeset (`StreamHostStats`, daemon-owned cadence,
`useHostStats` streaming hook, `HostStatsFooterAcceptance` Cypress).

## Design decisions (interview)

- **Semaphore:** daemon-global, **2** concurrent (no existing `tokio::sync::Semaphore` in the repo —
  established fresh here).
- **Compute scope:** disk **size only** is lazy / semaphore-gated / status-tracked; `git diff`
  (changed files, ±lines) stays eager.
- **Increments:** first snapshot frame, then per-worktree **status + final size** on `Calculating →
  Cached` (no live partial-byte growth).
- **RPC surface:** **add** `StreamWorktreeStats`, **keep** `ListWorktreesForProject`, **drop** the
  10-minute inspector poll in favor of lazy-on-visit (non-breaking).

## TODO

- [x] Create/update PRD documentation — `docs/ft/web/worktree-disk-usage-streaming.md`
- [x] Create changeset — this file
- [x] Acceptance tests (Plan-Red Step 6) — proto-free seams:
  - [x] Rust: `tests/worktree_size_calculator_acceptance.rs` (status model, transitions + timestamp,
        semaphore=2 concurrency, persistence-without-recompute, single-worktree isolation) — verified
        failing to compile on the missing `WorktreeSizeCalculator` API
  - [x] Cypress: `component/WorktreesScreenDiskUsage.cy.tsx` (None/Calculating/Cached + last-calc
        label, Recalculate all, per-row Calculate) — failing by construction (new test IDs absent)
  - [x] **USER REVIEW GATE** — acceptance tests approved
- [x] Red phase (Step 7) — proto-free unit tests:
  - [x] Rust: de-dup (enqueue-while-calculating is a no-op) + project snapshot, appended to the
        acceptance file
  - [x] Web: `src/lib/worktreeSize.test.ts` — `formatLastCalculated` + `applyWorktreeStatsEvent`
        reducer (bun:test), verified failing on the missing `./worktreeSize` module
  - [ ] Deferred to green (needs proto regen): proto `WorktreeSizeStatus` enum +
        `WorktreeRow.{size_status,size_calculated_at_unix_ms}`, `StreamWorktreeStats` +
        `CalculateWorktreeSize`; the `StreamWorktreeStats` handler test + `useWorktreeStatsStream`
        hook test
- [x] Green (proto-free increment) — implemented against the red tests:
  - [x] `tddy-daemon::worktrees::WorktreeSizeCalculator` (+ `WorktreeSizeStatus`/`WorktreeSizeState`/
        `WorktreeSizeUpdate`) — `tokio::sync::Semaphore` permit acquired inside each spawned walk,
        `Calculating`-then-`Cached` broadcast, de-dup, per-project `worktree_sizes.json` persistence
        (separate from `worktree_stats.json`). 7/7 tests pass; clippy/fmt clean.
  - [x] `packages/tddy-web/src/lib/worktreeSize.ts` (`formatLastCalculated` + `applyWorktreeStatsEvent`).
        8/8 bun tests pass.
  - [x] `WorktreesScreen` — Status + Last-calculated columns, per-row Calculate, screen-level
        Recalculate-all (presentational). Cypress verified by review (no node_modules to run here).
- [x] Green (wire-up increment) — done:
  - [x] proto `WorktreeSizeStatus` enum + `WorktreeRow.{size_status,size_calculated_at_unix_ms}`,
        `StreamWorktreeStats` + `CalculateWorktreeSize`; regen Rust (build.rs) + `connection_pb.ts` (buf).
  - [x] daemon: `WorktreeSizeCalculator` wired into `ConnectionService` (semaphore=2), `MpscWorktreeStatsStream`,
        `stream_worktree_stats` (snapshot + lazy enqueue + broadcast increments), `calculate_worktree_size`
        (membership-gated), `list_worktrees_for_project` overlays size fields; tonic adapter forwards.
        Red `stream_worktree_stats_rpc.rs` 4/4; no regressions (7+2+10); clippy/fmt clean.
  - [x] web: `useWorktreeStatsStream` hook + `WorktreesAppPage` on the stream; testkit
        `streamWorktreeStats`/`calculateWorktreeSize` stubs; `WorktreesStreamingAcceptance.cy.tsx` 4/4
        (Worktrees Cypress 12/12; bun unit 8/8).
- [ ] Follow-up (tracked): migrate `SessionWorktreeTab`/`useSessionWorktreeStats` off the 10-minute poll
      onto the stream (updates `SessionWorktreeTabAcceptance.cy.tsx`); optionally take the unary
      `ListWorktreesForProject` fully off the eager size walk.

## Implementation notes (green targets)

**tddy-daemon `worktrees.rs`:** new `WorktreeSizeCalculator` — `Arc`-shared, holds a
`tokio::sync::Semaphore` (permits injectable; default 2), an in-memory `Mutex<HashMap<(project,path),
WorktreeSizeState>>`, a `tokio::sync::broadcast` per-project update channel, an injectable
`Arc<dyn Fn(&Path) -> u64 + Send + Sync>` sizer (prod = `directory_size_bytes_best_effort`), and the
existing per-project stats-cache root for persistence. `enqueue` marks `Calculating` + broadcasts,
`spawn`s a task that acquires a permit and runs the sizer under `spawn_blocking`, then marks `Cached`
+ broadcasts + persists. De-dupes an already-`Calculating` worktree.

**ConnectionService:** `StreamWorktreeStats` mirrors `stream_host_stats` (auth → snapshot frame →
forward `subscribe()` increments over an mpsc-backed stream; box in the tonic adapter).
`CalculateWorktreeSize` mirrors `remove_worktree` (auth → path membership gate → `enqueue`).

**tddy-web:** `useWorktreeStatsStream` (mirrors `useHostStats`) feeds `WorktreesAppPage`;
`WorktreesScreen` gains the Status column + Recalculate-all + per-row Calculate; `SessionWorktreeTab`
+ `useSessionWorktreeStats` move from the 10-min poll to the filtered stream.

## Validation (pr-wrap)

- **Tests:** Rust `worktree_size_calculator_acceptance` 7/7; web `worktreeSize.test.ts` 8/8. Cypress
  `WorktreesScreenDiskUsage` (6) + `WorktreesScreen` (2) not executed here (no `node_modules`) —
  verified by review (per-row test-id alignment, status/size/last-calc rendering, callbacks, old
  spec preserved).
- **Lint:** `cargo fmt -p tddy-daemon --check` clean; `cargo clippy -p tddy-daemon --tests -D warnings`
  clean.
- **Risk review:** self-contained additions. `WorktreeSizeCalculator` holds no lock across `.await`,
  acquires the permit inside each spawned walk, de-dups in-flight walks, and persists best-effort
  (parse errors → empty, matching `list_cached_stats`). `WorktreesScreen` change is additive and
  presentational; the existing delete flow and `WorktreesScreen.cy.tsx` are preserved.
- **Production-readiness note:** this increment ships tested foundation code that is not yet wired to
  a user-facing RPC/screen path (the streaming wire-up is the tracked next increment) — consistent
  with the repo's "foundation first" changesets (e.g. session-catalog populate-only).

## Out of scope

Live partial-byte progress; lazy `git diff`; per-project semaphores; remote-host routing; removing
`ListWorktreesForProject`.
