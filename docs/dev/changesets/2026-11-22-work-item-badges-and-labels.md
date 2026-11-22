# 2026-11-22 — Agents label work items and mark them with keyed badges
**Type:** Feature

`#workqueue` 7/14 ([PR #1008](https://github.com/uppin/tddy-coder/pull/1008)). A work item records
**labels** (`WorkItem.labels`) and keyed **badges** (`WorkItem.badges`: key, text, tone of info / success /
warning / danger), written by three owner-only daemon exec tools (`work_item_set_labels`,
`work_item_set_badge`, `work_item_clear_badge`) and three RPCs (`SetWorkItemLabels`, `SetWorkItemBadge`,
`ClearWorkItemBadge`, each answering the queue after the change). Labels replace the set (trimmed, blanks
dropped, deduplicated); a badge under an existing key replaces it; limits (16 labels × 40, 8 badges,
60 characters of text) are refusals naming the limit. A row shows chips and badges by tone apart from the
derived status badge, the queue view filters by label, and an expanded row edits labels and clears a badge.
No activity is recorded for an annotation change.

Packages: tddy-changeset, tddy-work-queue, tddy-service, tddy-tool-engine, tddy-daemon-rpc, tddy-tools,
tddy-agent-launch, tddy-workqueue-web, tddy-client-web (and generated TS in tddy-rust-typescript-tests).

Validation and wrap: the tool-path refusals read as the RPC's do (they had answered `Debug`); a label
filter whose label vanished no longer empties the list; the owner's tool-list test and three
`session_list_enrichment` tests (written against the removed pr-stack recipe's manifest) now name the link
and annotation tools and a surviving recipe's manifest. The row's edit controls were added to meet the
changeset's Responsibility.

Code issues: `oversized-file-work-queue-ports` regressed 860 → 882 (three delegating one-liners) and
`oversized-file-test-util` 552 → 582 (three forwarders); neither is split — both files are stack-shared
with every later `#workqueue` node, so the split waits for the post-stack sweep. No backlog entry added or
cleared.

Feature docs: [pr-stacking.md § Labels and badges on a work item](../../ft/coder/pr-stacking.md#labels-and-badges-on-a-work-item-added-2026-11-22),
[prstack-package.md](../../ft/web/prstack-package.md).
