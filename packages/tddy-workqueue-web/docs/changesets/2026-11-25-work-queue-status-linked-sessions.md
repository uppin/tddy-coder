# 2026-11-25 — Work-queue status shows linked sessions and the item's branch
**Type:** Fix

`nodeChildSessionByIdentity` joins the plan's linked sessions after the recorded child (extending D39), so hydration, the status chip and the bound session follow a linked child; the row's link list names each link's host (`linkedSessionLabel`, `work-item-linked-session-<id>-<sessionId>`). Work item n7, PR #1038 — see [the cross-package entry](../../../../docs/dev/changesets/2026-11-25-work-queue-status-linked-sessions.md).
