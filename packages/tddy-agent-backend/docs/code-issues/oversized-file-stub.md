# oversized-file: stub.rs

**Location:** `packages/tddy-agent-backend/src/backend/stub.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` step 3.5 file-length gate on #522 (`#carve` 12/14), production lines counted to the first `#[cfg(test)]`
**Metrics:** **578 production lines** of 578 total (no `#[cfg(test)]`; the stub's tests live in the workflow suites that drive it) · budget 500 · **1.2× over**
**Thresholds breached:** length 578 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Moved:** 2026-09-23 — from `packages/tddy-core/src/backend/stub.rs` by `#carve` 12/14 (PR #522), with `git mv`
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 578 | first detection, at the file's new home. The size is inherited from `tddy-core`, not grown: the only edit is `use crate::workflow::ids::GoalId` → `use tddy_workflow::GoalId` (Cut 1), one line for one line |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | the canned per-goal responses inside `impl StubBackend` (L57–430), split by goal family | ~370 |
| B | the magic catch-words and helpers (L18–50) | ~35 |

`impl StubBackend` is almost the whole file, so the split is by goal inside it. Prove the seams with `restructure check --deep` before applying them.
