# Changeset: Moving a work item tells the queue's owner

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — more than one crate (kernel, session-activity, daemon-rpc, tool-engine, tools, telegram), a new activity source with ordering into the notification log, a restructure of an at-budget file first
**Stack**: `#workqueue` 8/14 — branch `feature/workqueue/move-activities`, base `feature/workqueue/badges-labels`
**PR**: [#1009](https://github.com/uppin/tddy-coder/pull/1009)

## Initial Discovery

[2026-11-22-move-activities-initial-discovery.md](./2026-11-22-move-activities-initial-discovery.md)

## Affected Packages

- **tddy-daemon-kernel**: `SessionNotificationSource::WorkQueue`; `is_a_sessions_own` true for it
- **tddy-session-activity**: `WatchKind::WorkQueue` (`ALL`, `as_str`, `parse`, `of`), matcher self-watch rule, log `wants`, storage DTO arms, `streams.rs` mapping (skipped by the drawer stream); split of `project_notification_log.rs` first
- **tddy-daemon-rpc**: publishing on reorder / set-parents (RPC and tool paths); `project_home_watches.rs` self-watch admission; `watch_wake.rs` `wants`
- **tddy-tool-engine**: `WATCH_KINDS` (8 → 9); `WorkQueueEvent` payload type
- **tddy-tools**: `monitor` help and kinds
- **tddy-telegram-control**: declines the kind

## Related Feature Documentation

- PRD: [PRD-2026-11-22-move-activities.md](../../ft/coder/1-WIP/PRD-2026-11-22-move-activities.md)
- Wrap updates: `docs/ft/daemon/session-notifications.md`, the watches section of `project-concept.md`

## Summary

Reorders and parent changes publish a `work_queue` activity to the queue's owner; its agent can watch
for them; its own moves do not wake it.

## Background

Exploration 3 §4D.2 lists every place a new kind touches (the `TrunkCommit` recipe); §4D.3 the
delivery paths (tool answers, idle wake, monitor).

## Responsibility

- `SessionNotificationSource::WorkQueue`, `WatchKind::WorkQueue` and every registry they join.
- `WorkQueueEvent { queue_session_id, item_id, change, actor }` with
  `WorkQueueChange::{Reordered, ParentsChanged}` and `Actor::{User, Agent}` — the kind's payload, which
  later nodes extend with new `WorkQueueChange` variants rather than new kinds.
- Publishing from every move path (UI RPC and tools).
- The owner self-watch rule and the "own moves do not wake" rule.
- Splitting `project_notification_log.rs` (499 lines) before adding the source.

## Boundaries

- Moves only: adding, deleting, annotating or linking items publish nothing.
- No web view of watches (separate TODO).
- Land / propagate / schedule events are n14's, as new `WorkQueueChange` variants.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n5` session-work-queue (#1006) | session-keyed queue; owner = queue session; reorder and set-parents addressed by session | publishes after a successful move, to the owner | change access, storage or the move operations themselves |
| `n6` pr-and-session-link (#1007), `n7` badges-labels (#1008) | line predecessors only | — | publish on link or annotation changes |

## Draft PR contract

Published (pass 2, commit 2): `tddy_session_activity::work_queue_activity::{WorkQueueEvent,
WorkQueueChange (Reordered, ParentsChanged), Actor (User, Agent), WorkQueueActivities::{new, publish_move},
wakes_owner}` and the failing tests below.

**Not in the contract, `/green`'s:** the `SessionNotificationSource::WorkQueue` and `WatchKind::WorkQueue`
variants. Adding a variant forces every exhaustive match across the ~12 registries (kernel, log, DTO,
streams, telegram, watch wake, admission, `WATCH_KINDS`, monitor) to change — implementation, not
surface — and the log must be split first (the ⛔ prerequisite). The tests reach the source and kind by
name (`"WorkQueue"`, `"work_queue"`) so they compile before the variants exist.

## Green wave

**Wave:** 4 of 8
**Greenable independently:** yes once n5 is in its base — the tests move items in n5's queue and read activities
**Concurrent with:** n6 pr-and-session-link, n7 badges-labels
**Blocks:** nothing in behaviour; n14 extends `WorkQueueChange`

Real dependency edges (the line is n1 · n2 · n3 · … · n14; `a → b` means b needs a's behaviour):

    n1 remove-pr-stack-recipe → n3 workitem-rename
    n2 github-event-inbox     → n4 session-pr-tabs, n6 pr-and-session-link
    n3 workitem-rename        → n5 session-work-queue
    n4 session-pr-tabs        → n11 commit-inbox-outbox
    n5 session-work-queue     → n6 pr-and-session-link, n7 badges-labels, n8 move-activities
    n6 pr-and-session-link    → n9 lineage-graph
    n9 lineage-graph          → n10 waves-line, n11 commit-inbox-outbox, n12 leak-linearity
    n10 waves-line            → n13 schedules
    n11 commit-inbox-outbox   → n13 schedules
    n13 schedules             → n14 land-propagate

Waves: 1 = n1, n2 · 2 = n3, n4 · 3 = n5 · 4 = n6, n7, n8 · 5 = n9 · 6 = n10, n11, n12 · 7 = n13 ·
8 = n14 (run along the line `1 1 2 2 3 4 4 4 5 6 6 6 7 8` — never steps back). Inside each wave the
blockers lead: n1 (12 transitive dependents) before n2 (8); n3 (10) before n4 (3); n6 (6) before n7
and n8 (0 each); n10 and n11 (2 each — tie, kept in plan order) before n12 (0). The sweep node is
reserved and planned after the pipelines drain.

## Prerequisites

### ⛔ BLOCKING — restructure `project_notification_log.rs` first

`packages/tddy-session-activity/src/project_notification_log.rs` is at 499 production lines (budget
500, Exploration 3 §4F); a new source arm takes it over. Split it (e.g. the `wants` policy and record
mapping into their own modules) via `/code-restructuring` before adding the arm. An engine refusal goes
to the operator before any hand move.

### Related — [2026-11-16-session-watches-are-not-shown-on-the-web.md](../todo/2026-11-16-session-watches-are-not-shown-on-the-web.md)

Unchanged; the new kind will appear there whenever watches get a view.

### Related — [2026-11-22-coder-target-back-channel-and-activity-cursor.md](../todo/2026-11-22-coder-target-back-channel-and-activity-cursor.md)

A target-hosted owner receives the activity through the same delivery paths; live verification stays
with that entry.

## Scope

- [x] Split `project_notification_log.rs`
- [x] Source, kind and payload in every registry
- [x] Publishing from UI and tool move paths
- [x] Owner self-watch; own moves do not wake
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

8 watch kinds; `is_a_sessions_own` = `Message | TrunkCommit`; self-watch admitted for `trunk_commit` only (`project_home_watches.rs:128`); moves publish nothing.

### State B (Target)

9 kinds; `work_queue` is session-own and self-watchable; a move publishes `WorkQueueEvent` with the
actor; `watch_matches` drops events whose actor is the watching session.

### Delta

As listed under Affected Packages.

## Implementation Milestones

- [x] Log split (no behaviour change)
- [x] Source/kind/payload registries
- [x] Publishing on moves
- [x] Watch rules + monitor

## Testing Plan

Integration tests in `tddy-session-activity` (matcher, log) and through the daemon (move → activity →
watch answer), mirroring `trunk_watch_acceptance.rs` and `session_watches_unit.rs`
(`the_kinds_are_named_as_the_tools_name_them` keeps the lists in step).

## Acceptance Tests

`packages/tddy-session-activity/tests/work_queue_activity_acceptance.rs`:
- [x] `a_user_reorder_becomes_an_activity_of_the_queue_owner`
- [x] `a_parents_change_names_the_parents_before_and_after`
- [x] `the_owners_own_moves_do_not_wake_it_and_anyone_elses_do`
- [x] `the_work_queue_watch_kind_is_named_work_queue`

"No other session's watch matches the queue" goes into `session_watches_unit.rs` with the variant
(`watch_matches` is crate-private).

`packages/tddy-daemon-rpc/tests/project_home_session_watch_acceptance.rs` (extended — it already holds the
self-watch rules and the host with a log and watches; the planned `work_queue_tools_acceptance.rs` never existed):
- [x] `an_owner_watching_work_queue_is_woken_by_a_user_move_from_the_queue_view` — a `ReorderWorkItem` on the
  owner's queue rides the owner's next answer as a `work_queue` event naming the operator

`packages/tddy-session-activity/tests/session_watches_unit.rs` (existing):
- [x] `the_kinds_are_named_as_the_tools_name_them` (now 9)

## Decisions & Trade-offs

- **One kind, `work_queue`, with a change enum** so later queue events (land, propagate, schedule
  fired — n14) need no new kind and no second pass through the ~12 registries.
- **Own moves don't wake the mover**: the activity is recorded (an audit trail) but not delivered to
  the session whose agent made it.
- **Moves = reorder and parents change** — the two operations that change an item's position; the
  operator's "moves" read narrowly, so adds and deletes stay quiet.

## Technical Debt & Production Readiness

- Own-agent moves are published with kind `Activity` (recorded, never matched by the owner's self-watch);
  moves by anyone else are `AttentionRequired`. The actor is carried by the kind, not a record field,
  to avoid touching ~127 `SessionNotification` literals. A future record field could replace it.
- Tool path publishes for `work_item_set_parents` only; the queue RPC path publishes for `ReorderWorkItem`
  (session-keyed and address-keyed). `RepointWorkItem` changes a base, not parents — not published.

## Refactoring Needed

None outstanding.

## Validation Results

`/validate-changes` (2026-11-23, [W]), base `origin/feature/workqueue/badges-labels` (c619c8ec8), leak check clean
(only n8's commits above n7).

- **Fixed — test-only branch (CRITICAL by CLAUDE.md):** `WorkQueueActivities::deliver` built a tokio runtime
  when called off one; only the sync `#[test]`s ever reached it. `publish_move` is now `async` and awaits the
  bus; the daemon-rpc publish chain (`publish_reorder`, `publish_parents_changed`, `reorder_work_item_in_store`,
  `announce_parents_changed`) awaits it, and the tests are `#[tokio::test]`.
- **Fixed — vacuous assertions:** two calls in `a_move_that_changed_nothing_publishes_nothing` were unawaited
  futures after the change; now awaited (clippy `-D warnings` clean).
- **Info:** `exec_tool/watch_wake.rs` is 537 lines; it was 535 at the base — not grown past 500 by this PR.
- Builds: `cargo check --all-targets` clean on all six touched packages; clippy `-D warnings` clean on
  tddy-session-activity, tddy-daemon-rpc.
- Stack boundary: no n6/n7 symbol implemented here; the rebase onto n7 moved reorder publishing into n7's
  `reorder_work_item_rpc` (`work_queue_queries.rs`), `ports.rs` left as n7 wrote it.

## TODO

- [x] Record initial discovery (`2026-11-22-move-activities-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [x] Validate changes (/validate-changes)
- [x] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
