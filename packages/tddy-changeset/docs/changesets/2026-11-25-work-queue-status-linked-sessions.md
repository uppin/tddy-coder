# 2026-11-25 — Work-queue status shows linked sessions and the item's branch
**Type:** Fix

`resolve_stack_node_branch` falls back through the item's `linked_sessions` in link order after the spawn-recorded child, reading each link's session directory under the local sessions root and skipping what is not there. Work item n7, PR #1038 — see [the cross-package entry](../../../../docs/dev/changesets/2026-11-25-work-queue-status-linked-sessions.md).
