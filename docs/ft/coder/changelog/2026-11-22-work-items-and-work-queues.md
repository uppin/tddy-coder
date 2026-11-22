# 2026-11-22 — Planned PRs are work items, stacks are work queues

The PR-stack vocabulary is renamed end to end: proto (`work_queue.WorkQueueService`, `AddWorkItem`,
`ListProjectWorkQueues`, …), the 17 agent tools (`pr_*` → `work_queue_*` / `work_item_*` /
`work_pr_*`), the `tddy-work-queue` crate, the `tddy-workqueue-web` package, components, the
`/sessions/project/:projectId/queue/:queueId` route and test ids. No aliases: an old tool or RPC name is
unknown. Behaviour and on-disk data are unchanged. See [PR stacking § Vocabulary](../pr-stacking.md#vocabulary--work-items-and-work-queues).
`#workqueue` 3/14 (#1004).
