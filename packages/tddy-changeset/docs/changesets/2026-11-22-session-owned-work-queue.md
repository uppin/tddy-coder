# 2026-11-22 — A session owns its work queue
**Type:** Feature

`session_work_queue` (`SessionWorkQueues`, `SessionWorkQueueRecord`, `starts_with_work_queue`, `NO_WORK_QUEUE_HINT`, `drop_old_project_stack_records`, the op-journal path); `WorkQueueStore` re-based as the project-scoped view over session queues; `read_session_stack` / `update_stack_atomic` read the session's own queue and never create one; `Changeset.stack` / `stack_id`, `read_stack_of` and the first-write path removed. `#workqueue` 5/14, PR #1006 — see [the cross-package entry](../../../../docs/dev/changesets/2026-11-22-session-owned-work-queue.md).
