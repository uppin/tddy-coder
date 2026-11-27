# oversized-file: session_agent_clone.rs

**Location:** `packages/tddy-session-agents/src/session_agent_clone.rs`
**Category:** oversized-file
**Detected:** 2026-10-01 — `/validate-changes` on PR #560 (`#agent-worktree` 1/4); the file is not edited by that PR (its `master` count is 1,157)
**Metrics:** **1,158 production lines** (counted to the first `#[cfg(test)]`; 1,257 total) · budget 500 · **2.3× over**
**Thresholds breached:** length 1,158 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed** · unchanged by #633 (1,157 → 1,156) · **regressed by #1048** (`#fs-port` 4/13, 1,156 → 1,160, still 2.3× over; see the 2026-11-27 row — the split stays with this record's follow-up)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-01 | 1,158 | first record. Recorded because PR #560's boundaries ("no peer-clone changes") put this file next to the change, so a later node that does touch it should know the size it starts from |
| 2026-10-04 | 1,157 | 1,158 → 1,157 after `#e2e-leg` 2/5 (#579): `SessionAgentCloneSpec.common_room_slot` becomes `common_room` and holds a `CommonRoom`. Not grown; unchanged class |
| 2026-10-20 | 1,156 | 1,157 → 1,156 after `#satellite` 11/13 (#633): the clone joins through `RoomConnector` and self-mints through `RoomTokenMinter`, and the join builder `clone_room_join` lives in `room_access.rs`. Not grown. The module names no LiveKit crate; the two-sides split is still open, and #633's Boundaries forbid it |
| 2026-11-27 | 1,160 | 1,156 → 1,160 by #1048 (`#fs-port` 4/13): the mirror-marker write goes through `tddy_fs::DiskFs` (+4/−1: the `use tddy_fs::Fs as _` line, the two-line justifying comment, and the longer `DiskFs.write` call — the mirror below is opened on the real disk by `tddy-session-sync`, so the marker must land on the same filesystem; `cargo fmt` in the same stage +1). Incidental port growth, not logic; the lifecycle-vs-hosted-clone seam is still undesigned. Deferred to the `#fs-port` sweep node (see `docs/dev/todo/2026-11-27-fs-port-oversized-files-grown-while-over.md`) |

## What the tool found

`wc -l` to the first `#[cfg(test)]`. Not measured: complexity, nesting, CRAP. The module is the
clone model for a remote agent's checkout — one per (session, owning daemon) — and holds the clone
lifecycle, readiness and the hosted-clone path together.

## What would close it

Not designed. The lifecycle (create / ready / retire) versus the hosted-clone serving path is the
first reading of a seam; prove it with `restructure check --deep` before applying. Not touched by
PR #560, so no deferral decision is attached to this change.
