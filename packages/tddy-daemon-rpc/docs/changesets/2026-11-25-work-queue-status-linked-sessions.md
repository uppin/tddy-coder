# 2026-11-25 — Work-queue status shows linked sessions and the item's branch
**Type:** Fix

The link tool stores the session's host, and link answers list `{ session_id, daemon_instance_id }`; `work_queue_status` lists every linked session with its host per node and reports the effective branch (stored, else a linked session's, else the assigned PR's), resolved for display and never written back. Work item n7, PR #1038 — see [the cross-package entry](../../../../docs/dev/changesets/2026-11-25-work-queue-status-linked-sessions.md).
