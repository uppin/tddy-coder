# 2026-11-24 — The work-queue view stays current

A queue changed from outside its open view (an orchestrator's `work_item_add` or `work_item_link_session`,
another session or view, a schedule firing) now shows in the view without leaving it, through
`WatchWorkQueue` on the host that holds the queue. Every write that changes a queue raises its
`revision`, and the view keeps the newest copy, so a late snapshot never undoes the operator's own edit.
A queue that cannot be loaded says why instead of reading as empty. A dropped watch keeps the queue
shown, reconnects with backoff and resyncs. Item status (`StreamWorkQueueStatus`) also follows queue
changes. See [PR stacking § The queue view stays current](../pr-stacking.md#the-queue-view-stays-current-added-2026-11-24).
