# 2026-11-22 — A session owns its work queue
**Type:** Feature

Listing fills `SessionEntry.owns_work_queue` and `work_queue_id` from the session's own queue, read once per row (`work_queue_json_of`). `#workqueue` 5/14, PR #1006 — see [the cross-package entry](../../../../docs/dev/changesets/2026-11-22-session-owned-work-queue.md).
