# 2026-11-22 — A work item takes an existing PR and linked sessions

A queue's agent can assign an existing pull request to a work item and link sessions to it — and undo
either — with four new work tools; a linked session can read the queue. The PR's branch becomes the
item's, and an item already on another branch is refused. PR status is read through the daemon's GitHub
inbox, so repeated readers cost no extra GitHub requests. See
[PR stacking § An item's assigned PR and linked sessions](../pr-stacking.md#an-items-assigned-pr-and-linked-sessions-added-2026-11-22)
(`#workqueue` 6/14, PR #1007).
