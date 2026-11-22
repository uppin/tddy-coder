# 2026-11-22 — A work item takes an existing PR and linked sessions
**Type:** Feature

`item_ops::item_links` (`assign_pr`, `unassign_pr`, `link_session`, `unlink_session`, `ItemLinkError` with `Display`); `inbox_reads` (PR status, queue status views, item check summaries with their runs, and `StreamWorkQueueStatus` snapshots read through the GitHub inbox); `WorkQueueHandler` gains the four link RPCs and `stream_work_queue_status` (`WorkQueueStatusStream`); `pr_insight::pull_number_for_node` prefers the assigned PR. `#workqueue` 6/14, PR #1007 — see [the cross-package entry](../../../../docs/dev/changesets/2026-11-22-work-item-pr-and-session-links.md).
