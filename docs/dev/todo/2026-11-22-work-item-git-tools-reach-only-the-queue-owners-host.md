# 2026-11-22 — Work item git tools reach only the worktrees on the queue owner's host

**Category:** Known gap
**Source:** `#workqueue` 5/14 (session-work-queue, PR #1006), found at its wrap

A work queue lives with the session that owns it, on that session's host
(`<sessions>/<session_id>/work-queue.yaml`). The six work item git tools (`work_item_merge`,
`work_item_repoint`, `work_item_close`, `work_item_resolve_conflicts`, `work_item_set_parents`,
`work_item_spawn_session`) run on the host that holds the queue, in the items' worktrees **on that
host**. Only the queue's owner may call them, and the owner's queue is always on the owner's own host.

So an orchestrator on host-a whose queue's items have their sessions and worktrees on host-b has no
way to merge, repoint or rebase those items with the git tools: host-a holds the queue but not the
worktrees, and host-b holds the worktrees but not the queue. Before `#workqueue` 5/14 a stack was a
project record that could sit on host-b beside the worktrees, and the call was forwarded there.

The cross-host test of the six tools (`work_item_git_tools_acceptance`) proves only the case that
exists: a call naming a queue on another host is forwarded and refused there to a session the queue
merely addresses. A test of the six tools *acting* across hosts needs this entry closed first.

## What would close it

Let the holding host run a git step in an item's worktree on another host — forward the per-item
git step (not the whole tool call) to the host that holds the item's session, with the queue read and
written on the owner's host — and test the six tools acting with an owner on host-a and items on
host-b.

## Recommendation

Leave until the stack's lineage and land/propagate nodes (`#workqueue` 9/14 and 14/14) settle how
items on several hosts are driven: they reshape the same path.
