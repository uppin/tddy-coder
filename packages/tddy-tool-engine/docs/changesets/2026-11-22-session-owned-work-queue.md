# 2026-11-22 — A session owns its work queue
**Type:** Feature

`work_queue_access` (`WorkQueueAccess { Owner, Addressed }`, `work_queue_access_of`, `work_queue_tools_offered_to`, `names_a_queue_on_another_host`); `caller_records` reads access from queues (`access_to_queue_here`, `with_work_tools_offered_to`); `work_queue_create` removed; `StartSession` schema `with_work_queue`. `#workqueue` 5/14, PR #1006 — see [the cross-package entry](../../../../docs/dev/changesets/2026-11-22-session-owned-work-queue.md).
