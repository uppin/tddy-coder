# 2026-11-23 — Moving a work item tells the queue's owner
**Type:** Feature

`#workqueue` 8/14 ([PR #1009](https://github.com/uppin/tddy-coder/pull/1009)). A reorder in the queue view
(`ReorderWorkItem`, session- or address-keyed) and an agent's `work_item_set_parents` publish a `work_queue`
activity (`SessionNotificationSource::WorkQueue`) on the session that owns the queue — one line naming the item,
the change (positions 1-based, or parents before and after) and who made it. The owner can watch for it with
`CreateSessionWatch` / `tddy-tools monitor` kind `work_queue` — the second self-watch after `trunk_commit`;
watch kinds 8 → 9. A move by the operator or another session's agent is `ATTENTION_REQUIRED` and wakes an idle
owner; the owner's own agent's move is `ACTIVITY`, kept in the project log and never delivered. The source is
session-own: Telegram, the drawer, Tddy Desktop and `ReadProjectNotifications` skip it. Adds, deletes,
annotations and links publish nothing.

First, `project_notification_log.rs` was split (499 → 448 production lines; paging into
`project_notification_log/paging.rs`) so the new source fit the budget.

Packages: tddy-daemon-kernel, tddy-session-activity, tddy-daemon-rpc, tddy-tool-engine, tddy-tools,
tddy-telegram-control.

Wrap: the publisher awaits the bus (a runtime-less branch only sync tests reached was removed); the daemon-level
test lives in `project_home_session_watch_acceptance.rs` (`an_owner_watching_work_queue_is_woken_by_a_user_move_from_the_queue_view`).
Rebased onto n7 at wrap: reorder publishing sits in n7's `reorder_work_item_rpc`.

Backlog: none added or resolved. The actor is carried by the notification kind, not a record field (a field
would touch ~127 `SessionNotification` literals); `RepointWorkItem` changes a base, not parents, and publishes
nothing.
