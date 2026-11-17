# oversized-file: session_watches.rs

**Location:** `packages/tddy-session-activity/src/session_watches.rs`
**Category:** oversized-file
**Detected:** 2026-11-17 — `/pr-wrap` step 3.5 file-length gate on #930 (`#activity-wake` 2/3), production lines counted to the first `#[cfg(test)]`
**Metrics:** **0 → 650 production lines** (2026-11-17) · budget 500
**Thresholds breached:** length 650 > 500
**Restructure:** `move_item` / `extract_module` into submodules — `/code-restructuring` territory
**Status:** Open — deferred to a follow-up branch after the `#activity-wake` stack lands; unclaimed

Not split in #930: successor PR #931 (`feature/activity-wake/idle-wake`) also modifies this file
(it extracts `take_delivery`'s body as `apply_delivery` for its `deliver_with`), so the split is
deferred under `/pr-wrap` step 3.5's stack-branch rule.

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-11-17 | 0 → 650 | #930: the watch store, matcher and endings in one new module |

## What would close it

Move the matcher (`WatchKind::of`, `watch_matches`) into `session_watches/matcher.rs` and the
persistence (`SessionWatchStore::open`/`persist`, `watches_file`, `ProjectWatches`) into
`session_watches/storage.rs`, as the node's changeset planned, with `tddy-tools restructure`
intents proved by `restructure check --deep`, and confirm the file and each new module are each
≤ 500 production lines.
