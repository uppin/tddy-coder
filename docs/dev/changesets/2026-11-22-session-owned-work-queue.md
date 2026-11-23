# 2026-11-22 — A session owns its work queue
**Type:** Feature

`#workqueue` 5/14 ([PR #1006](https://github.com/uppin/tddy-coder/pull/1006)). Work queues are owned by
sessions: one queue per owning session, stored in its directory (`<sessions>/<id>/work-queue.yaml`), whose
id is the session's id. A queue is created only at start — asked for with `with_work_queue` (proto, MCP
`StartSession`, the New-session form) or implicitly for an orchestrator-role start (also given on the next
resume of an orchestrator that predates queues). Work-tool access follows the queue, never a role: the
owner has every tool, a session an item links to has the two reads, everyone else nothing; listing and
execution read the same `WorkQueueAccess`, and a call naming a queue on another host is decided by that
host. Addressing a session with no queue is refused with the hint "a queue is created when a session is
started with a work queue". The inspector gains a Work queue tab and opens on it. Project stack records
under `<projects>/*/stacks/` are deleted at daemon start, not migrated; `Changeset.stack`,
`Changeset.stack_id`, the startup migration sweep, the first-write creation path, `CreateWorkQueue`,
`work_queue_create`, the web "New stack" action and the `/sessions/project/:id/queue/:id` route are gone.

Packages: tddy-changeset, tddy-work-queue, tddy-daemon, tddy-service, tddy-agent-launch, tddy-tool-engine,
tddy-daemon-rpc, tddy-session-lifecycle, tddy-session-activity, tddy-workqueue-web, tddy-session-web,
tddy-session-create-web, tddy-client-web, tddy-tools.

Deliberate deviations from the plan: `WorkQueueAddress` keeps `queue_id` (= the owning session's id) rather
than a `session_id` field; `WorkQueueStore` remains as the project-scoped view over the session queues.

Backlog: the 2026-11-14 entry "A node session is not listed its stack reads by its own host when its stack
is on another host" was narrowed, not resolved — serving across hosts works, the listing gap stays. New
entry 2026-11-22 "Work item git tools reach only the worktrees on the queue owner's host".

Code issues: `oversized-file-work-queue-ports` touched and better (876 → 860 production lines, still open);
`complexity-exec-tool-ports-list-exec-tools` not touched (72 lines).

Feature docs: [pr-stacking.md](../../ft/coder/pr-stacking.md), [project-concept.md](../../ft/daemon/project-concept.md),
[session-drawer.md](../../ft/web/session-drawer.md), [session-pr-inspector.md](../../ft/web/session-pr-inspector.md),
[url-state-routing.md](../../ft/web/url-state-routing.md), [create-session-package.md](../../ft/web/create-session-package.md).
