# A session owns a work queue — PRD

**Date**: 2026-11-22
**PRD Type**: Feature (with a data removal)
**Stack**: `#workqueue` 5/14 — branch `feature/workqueue/session-work-queue`, base `feature/workqueue/session-pr-tabs`

## Affected Features

- **Primary Feature**: work queues (the renamed [PR stacking](../pr-stacking.md) doc after n3)
- [Project concept — orchestrator sessions](../../daemon/project-concept.md)
- [Session drawer](../../web/session-drawer.md) — inspector Work queue tab
- New-session form ([create session package](../../web/create-session-package.md))

## Summary

A **work queue belongs to a session**, identified by the session's id. Any session can own one; none
is decided by role.

- **How a queue comes into being:** at session start. The `StartSession` tool and the web New-session
  form offer **"with work queue"**; a session started with it owns an empty queue from its first
  moment. A session started as an orchestrator (the project's main session and every other
  orchestrator) gets its queue implicitly at creation — the one place a role is consulted. A session
  started without the option has no queue: no Work queue tab, and nothing to address.
- **Who can do what:** the owning session's agent and the user add, edit, reorder and remove work
  items. A session a work item is linked to can read that queue. Every `work_*` tool is offered to,
  and accepted from, a session **because it owns or is addressed by a queue** — never because of its
  role.
- **Addressing a session that has no queue** is refused: *"session <id> has no work queue — a queue
  is created when a session is started with a work queue"*.
- **Inspector:** a session that owns a queue shows a **Work queue** tab, and the inspector opens on it.
  Otherwise PR when the session has a PR, otherwise Worktree (n4).
- **Old stacks are dropped.** Project stack records from before this change are deleted on upgrade;
  there is no migration.

## Background

Today stacks are project records (`<data>/projects/<project_id>/stacks/<stack_id>.yaml`), many per
project, reached by `Changeset.stack_id`; tools are gated by `is_orchestrator_role`. The operator's
model: work is queued by the session that owns it, any session may own a queue, and access follows the
queue — not a role (operator amendment, 2026-11-22).

## Proposed Changes

### What's Changing

- Storage: one queue per session, `<data>/sessions/<session_id>/work-queue.yaml`, created at start;
  removed with the session.
- `StartSessionRequest.with_work_queue` and the MCP `StartSession` argument `with_work_queue`; a
  "With work queue" checkbox in the New-session form; orchestrator starts create one implicitly.
- Queue addressing by session id (`WorkQueueAddress { session_id, daemon_instance_id }`) on every RPC
  and tool; an omitted queue means the caller's own.
- Tool access by ownership or addressing; listing offers the tools on the same basis.
- `CreateWorkQueue` and `work_queue_create` are removed (a queue is created only at start).
- Inspector Work queue tab hosting `WorkQueueView`; the default-tab precedence.
- The drawer's project row lists the project's queues by owning session and opens that session's
  Work queue tab (the separate `/queue/:id` route is removed).
- Upgrade: old project stack records (and their `.op.json` journals) are deleted at daemon start; the
  `Changeset.stack` legacy field, `Changeset.stack_id`, the startup migration sweep and the
  pre-store fallbacks are removed.

### What's Staying the Same

- Work item fields and item operations (add, update, delete, reorder, repoint, merge, …) — only their
  addressing changes.
- Cross-host forwarding by `daemon_instance_id`.

## Impact Analysis

### Technical Impact

A storage re-key and a data removal; proto addressing change; a gating rewrite in tool-engine and
daemon-rpc; a start-path option in agent-launch, the MCP schema and the web form; an inspector tab.

### User Impact

The New-session form has a "With work queue" option. A queue opens as a tab of its session. Existing
stacks disappear on upgrade (accepted by the operator).

## Implementation Plan

One node, `feature/workqueue/session-work-queue` (#workqueue 5/14).

## Acceptance Criteria

- [ ] A session started with a work queue owns an empty queue; one started without has none.
- [ ] An orchestrator session owns a queue from creation without the option.
- [ ] The owning agent adds a work item without naming the queue; the user adds one from the tab.
- [ ] A session linked to an item reads that queue and cannot change it.
- [ ] A `work_*` call addressing a session with no queue is refused with the hint.
- [ ] Tool listing and acceptance never consult a role.
- [ ] The inspector of a queue-owning session shows and opens on Work queue; others show no such tab.
- [ ] Old project stack records are gone after the first start of the new daemon.
