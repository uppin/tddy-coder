# 2026-11-22 — A session owns its work queue
**Type:** Feature

`queue_migration` (the startup migration sweep) removed; item ops write the session's own queue. `#workqueue` 5/14, PR #1006 — see [the cross-package entry](../../../../docs/dev/changesets/2026-11-22-session-owned-work-queue.md).
