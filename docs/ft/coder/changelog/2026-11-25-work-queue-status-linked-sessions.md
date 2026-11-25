# 2026-11-25 — Work-queue status shows linked sessions and the item's branch

`work_queue_status` lists every session linked to an item — with the host that holds it — instead
of only the legacy spawn-recorded one, and reports the item's actual branch: what it records,
else a linked session's branch, else its assigned PR's head branch. The queue view follows the
same links for the row's child session, branch line and status chip, and names each link's host.
Linking a session on another host now records that host
(`work_item_link_session { item_id, session_id, session_daemon_instance_id? }`). See
[PR stacking § An item's assigned PR and linked sessions](../pr-stacking.md#an-items-assigned-pr-and-linked-sessions-added-2026-11-22)
(PR #1038).
