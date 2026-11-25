# 2026-11-25 — Work-queue status shows linked sessions and the item's branch
**Type:** Fix

`work_item_link_session` takes the optional `session_daemon_instance_id` naming the session's host (`daemon_instance_id` already names the queue's); `WorkQueueOp::LinkSession` carries it. Work item n7 — see [the cross-package entry](../../../../docs/dev/changesets/2026-11-25-work-queue-status-linked-sessions.md).
