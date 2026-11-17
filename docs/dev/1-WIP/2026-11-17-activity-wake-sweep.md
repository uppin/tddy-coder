# Changeset: The activity-wake sweep pays down the stack's structural debt

**Date**: 2026-11-17
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 3 — three single-package file splits by `move_item`/`extract_module` (session-activity's watch store, its log storage, the daemon's mount host), a recorded green baseline, no behaviour change, no new surface
**Stack**: `#activity-wake` 6/6 (sweep) — branch `feature/activity-wake/sweep`, base `feature/activity-wake/session-messages` (#935)

## Initial Discovery

[2026-11-17-activity-wake-sweep-initial-discovery.md](./2026-11-17-activity-wake-sweep-initial-discovery.md)

## Prerequisites

None open in the packages this node touches beyond the backlog below (each record is `Open … unclaimed`; nothing carries a `**Claimed by:**` line).

## Affected Packages

- **tddy-session-activity** — split `src/session_watches.rs` (857 lines) into submodules; split `src/project_notification_log/storage.rs` (578) the same way
- **tddy-daemon** — split `src/runtime/mount_host.rs` (527)

## Related Feature Documentation

- [Project concept](../../ft/daemon/project-concept.md) — unchanged; the sweep moves no behaviour

## Summary

The `#activity-wake` stack left three files over its 500-line budget behind, each recorded by
`/pr-wrap`'s file-length gate and each deferred under the stack-branch rule because a successor
also touched it. Nothing downstream shares them any more: this node is the tail. It splits each
file into submodules by `move_item`/`extract_module` — mechanical moves the restructure engine
applies, no code written by hand — deletes the three records with the final measurements in the
change-history entries, and changes no behaviour.

## Final measurements

- `packages/tddy-session-activity/src/session_watches.rs`: **857 → 205 production lines** (budget 500) — split into `session_watches/{matcher 145, delivery 211, store 349, stream 428}` (`stream.rs` pre-existing from the #935 wrap). This closes `packages/tddy-session-activity/docs/code-issues/oversized-file-session-watches.md`, deleted in the commit that closed it. The function-ceiling side-note inside the record (`take_stream_batch` 255, `monitor_cli` `stream` 62) is NOT closed by this node.

## Scope

- [x] `session_watches.rs` 857 → submodules under `session_watches/`, each file under budget; the records' close conditions are file-length-only; the function-ceiling side-notes (`take_stream_batch` 255, `monitor_cli` `stream` 62) are left open in the report — `extract_method` there would rewrite bodies, outside a move-only sweep
- [ ] `project_notification_log/storage.rs` 578 → submodules under `project_notification_log/storage/`
- [ ] `mount_host.rs` 527 → submodules under `runtime/mount_host/`
- [ ] The three `oversized-file-*` records deleted by the commits that close them, final measurement recorded first
- [ ] No TODO added; the stack's one open TODO (`2026-11-16-session-watches-are-not-shown-on-the-web.md`) is NOT this node's — it goes to the TODO gate

## Responsibility

This PR owns **three mechanical splits and nothing else**: the module moves above, the `mod`/`use`
wiring they need, and the record deletions. Any behaviour change, signature change or
public-surface change is a stop. The function-ceiling side-notes recorded inside the oversized-file
records are NOT closed by this node.

## Boundaries

- **No behaviour change** — the moved items keep their bodies, byte for byte, up to visibility and import wiring.
- **No new surface** — nothing new is `pub` that wasn't; `pub(crate)`/`pub(super)` widening only where a submodule split forces it.
- **No TODO** may be added by this node; it either does the work or stops.
- **Not this node's:** the web-visibility TODO (the user's gate decides), the `pty_spawn` and `build_session_host` function-complexity records (pre-existing, not worsened by the stack), sibling addressing (a later feature node).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| every node (#929–#935) | the finished code this node restructures | moves it, unchanged | change any of it |

## Green baseline

Recorded before the first move, on `origin/feature/activity-wake/session-messages` (8736ce533), scoped to the three touched packages:

- `./dev cargo test -p tddy-session-activity` — 29 suites, green
- `./dev cargo test -p tddy-daemon --lib` — green
- `./dev cargo clippy -p tddy-session-activity -p tddy-daemon --all-targets -- -D warnings` — clean
- `cargo fmt --all --check` — clean

The same suites are the after-gate: green before and after, and CI on the ready PR confirms the whole workspace.

## Draft PR contract

`Type: Refactor` — the contract is this plan plus the green baseline above, with a `plan.jsonl`
of `move_item`/`extract_module` intents per `code-restructuring`. No failing tests: a recorded
green baseline stands in for them (the suites covering the moved code, green before and after).

## Green wave

**Wave:** 6 of 6
**Greenable independently:** yes — it depends on no behaviour, only on the files existing.
**Concurrent with:** none
**Blocks:** none

## Technical Changes

### State A (Current — on `feature/activity-wake/session-messages`)

- `packages/tddy-session-activity/src/session_watches.rs`: 857 production lines — the watch store, matcher, kinds, delivery, stream lease and records in one file; `take_stream_batch` 255 lines, `stream` 62 (function ceiling 60).
- `packages/tddy-session-activity/src/project_notification_log/storage.rs`: 578 lines — the log's DTOs, persistence and replay.
- `packages/tddy-daemon/src/runtime/mount_host.rs`: 527 lines — the mount host's builders and wiring.
- Three `oversized-file-*` records open, unclaimed.

### State B (Target)

- The same items split into submodules beside their parents (`session_watches/{store,matcher,delivery,stream,…}.rs`, `storage/{dto,persist,replay}.rs`, `mount_host/{builders,wiring}.rs` — the exact seams come from the engine's anchor resolution at plan time), every file and function under budget.
- The three records deleted, each by the commit that closes it, with the final measurement recorded in the change-history entry first.

## Implementation Milestones

- [ ] plan.jsonl authored from the engine's anchors
- [ ] session-activity's two splits applied and green
- [ ] tddy-daemon's split applied and green
- [ ] Records deleted with final measurements
- [ ] Baseline suites green after; clippy/fmt clean

## Testing Plan

### Testing Strategy

A restructure changes no behaviour, so its test is the recorded baseline: the three packages' own suites, green before the first move and green after the last, plus CI's whole-workspace run on the ready PR.

### Acceptance Tests

None — `Type: Refactor`, no new behaviour, nothing to write red.

### Unit Tests

Existing suites, re-run: `tddy-session-activity` (29 suites, among them `session_watch_stream_unit`, `session_messages_unit`), `tddy-daemon --lib`, and the parent suites that pin the moved code's callers.

### Testing Principles Applied

Real suites, not snapshots; the baseline recorded before the first move; every apply gated on the engine's compile check.

## Technical Debt & Production Readiness

- The restructure engine's anchor resolution needs the warm index daemon (`./run-index-daemon`); if it does not answer, this node stops rather than hand-moving (the #935 wrap's hand move is the recorded exception that motivates this node, not a licence to repeat it).

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## Successor PRs

None — the stack's tail.

## TODO

- [x] Record initial discovery (`2026-11-17-activity-wake-sweep-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [ ] Create failing acceptance tests — N/A (`Type: Refactor`; green baseline recorded instead)
- [ ] USER REVIEW — the refactor plan (this changeset + plan.jsonl)
- [ ] TDD Red — N/A (baseline)
- [ ] TDD Green — apply plan.jsonl via the restructure engine
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p tddy-session-activity -p tddy-daemon`) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
