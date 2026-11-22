# 2026-11-22 — A work item takes an existing PR and linked sessions
**Type:** Feature

The four link RPCs (`work_queue/links.rs`) and `StreamWorkQueueStatus` (`work_queue/status_stream.rs`); the link tools served by the exec-tool host; `GetPrStatus`, `QueryBranch`'s PR leg and `work_queue_status` read PRs through the GitHub inbox with the project's credential (`SessionPullRequests`), the status path now async; the test daemon forwards the new RPCs. `#workqueue` 6/14, PR #1007 — see [the cross-package entry](../../../../docs/dev/changesets/2026-11-22-work-item-pr-and-session-links.md).
