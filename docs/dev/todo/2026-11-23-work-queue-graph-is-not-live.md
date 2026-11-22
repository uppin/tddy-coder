# The work-queue graph is not live

**Added:** 2026-11-23 — `#workqueue` 9/14 (#1010)
**Area:** tddy-workqueue-web `WorkQueueGraphPanel`, tddy-daemon-rpc `work_queue/status_stream.rs`

`WorkQueueGraphPanel` reads `GetWorkQueueGraph` when the queue view opens and after each queue edit
(`refreshKey`). A PR retargeted on GitHub moves its line edge only on the next read; the view does not
redraw on its own.

**Why deferred:** the status stream (`StreamWorkQueueStatus`) already reacts to the inbox's PR events for
check counts; carrying line edges on it is a proto + stream change that n9's scope (the lineage read and
its tools) did not own.

**What would close it:** add line edges to `WorkQueueStatusView` (or trigger a graph re-read on each status
snapshot) so an inbox PR-base change redraws the graph.
