# 2026-11-23 — Work items can be armed to merge or propagate, with every condition shown live
**Type:** Feature

`#workqueue` 13/14 ([PR #1014](https://github.com/uppin/tddy-coder/pull/1014)). `WorkQueue.schedules`
stores armed merges and propagations; `tddy_work_queue::schedule` evaluates each into met/waiting/blocked
conditions (merge: order, current, checks, review with `skip_review`; propagate: inbox, conflicts, the
base's checks, worktree), a worst-condition status and `held_up_on`. Four RPCs and tools
(`work_item_schedule`, `work_queue_schedule_wave`, `work_item_unschedule`, `work_queue_schedules`);
`WorkQueueStatusView.schedules` on the status stream; a `SchedulesPanel` in the queue view. Nothing fires.

Packages: tddy-changeset, tddy-work-queue, tddy-service, tddy-daemon-rpc, tddy-tool-engine, tddy-tools,
tddy-agent-launch, tddy-client-web, tddy-workqueue-web (plus `..Default::default()` in test literals of
other packages).

Wrap: rebased onto n12; the status stream carries chain health and schedules together. `schedules.rs`
(694 production lines) split into `schedules/{mod,facts,evaluate,rpc}.rs`. No `threads` condition —
[2026-07-30 follow-ups](../../../../docs/dev/todo/2026-07-30-pr-stack-full-control-follow-ups.md) (GraphQL).
