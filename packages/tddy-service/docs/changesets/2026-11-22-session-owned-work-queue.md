# 2026-11-22 — A session owns its work queue
**Type:** Feature

`StartSessionRequest.with_work_queue` (51), `SessionEntry.owns_work_queue` (44), `CreateWorkQueue` removed; `WorkQueueAddress.queue_id` documented as the owning session's id. `#workqueue` 5/14, PR #1006 — see [the cross-package entry](../../../../docs/dev/changesets/2026-11-22-session-owned-work-queue.md).
