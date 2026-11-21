# 2026-11-21 — The Worktree tab shows a target session's worktree

**Type:** Feature · **Stack:** `#target-parity` 7/7 (PR #975) · **Packages:** `tddy-worktree-service` ·
Feature: [session-worktree-inspector.md § On a target session](../../ft/web/session-worktree-inspector.md#on-a-target-session),
[worktree-disk-usage-streaming.md](../../ft/web/worktree-disk-usage-streaming.md)

## Problem

`StreamWorktreeStats` listed only the worktrees of the project's clone on the daemon host. A target
session's worktree (`/home/coder/…/.worktrees/<id>`) was never among them, so its Worktree tab found
no row, showed it as missing and offered Restore (Coder probe, session `01a1fcba-…`).

## What changed

- **`target_worktree_stats.rs`** (new): `target_sessions_of_project`; `add_target_rows` puts a
  `CALCULATING` row per target session into the stream's first snapshot and measures each in the
  background through n2's `TargetShells` port (`git branch --show-current`, `git diff --numstat
  HEAD`, `du -sk`), bounded by the request timeout; failure or timeout → a `stale` row with no
  numbers. `recalculate_target_row` serves `CalculateWorktreeSize` of a target path and publishes
  on `TargetRowBus`, a per-project `broadcast` the streams forward.
- **`target_worktrees.rs`**: `run_on_target` answers stdout; `target_session_from` is shared.
- **`service.rs`**: the two call sites, `sessions_dir_of` and the bus accessor; the free helpers moved
  to **`service_helpers.rs`** (by hand — a pure move of five free functions).

## Decisions

- **Target rows bypass `WorktreeSizeCalculator`**: it walks local directories under a semaphore and
  persists results; a target row is three remote commands, measured per stream and never persisted.
- **No proto or web change**: the tab already matches its row by path.

## Wrap-time measurements

- `service.rs`: **883 → 831** production lines (−52; n7's own additions plus the helper split). Still
  over budget; recorded in `packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md`.
- `target_worktree_stats.rs` (new): 229 production lines; `service_helpers.rs` (new): 100.

## Tests

`packages/tddy-worktree-service/tests/target_worktree_stats_acceptance.rs` (4 tests; the Calculate test
changes the target after the stream's own measurement, so it fails without the target path — checked
by disabling `recalculate_target_row`). Shares `tests/target_session_support/` with
`target_worktree_actions_acceptance.rs`.
