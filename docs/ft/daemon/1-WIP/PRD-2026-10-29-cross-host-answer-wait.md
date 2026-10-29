# PRD: A child waits for an orchestrator on another host

**Created:** 2026-10-29
**Status:** 🚧 Planned
**Area:** daemon — project home (`docs/ft/daemon/project-concept.md`, `docs/ft/daemon/claude-cli-session.md`)
**Stack:** `#home-inbox 10/11`
**Resolves:** [`docs/dev/todo/2026-10-29-permission-wait-needs-a-same-host-orchestrator.md`](../../../dev/todo/2026-10-29-permission-wait-needs-a-same-host-orchestrator.md)

## Problem

A claude-cli child's hook waits for its orchestrator's answer to a permission prompt (`6/11`) or a
question (`8/11`). The wait happens only when the orchestrator runs on the child's own host.

A child whose changeset names an orchestrator on another daemon (`orchestrator_daemon_instance_id`)
gets `NO_ORCHESTRATOR` at once. Claude then shows its own prompt.

The answer path already crosses hosts. `AnswerSessionPermission` and `AnswerSessionQuestion` are
target tools, forwarded to the child's host, where the waiting hook is held. Only the presence
check refuses: the child's host cannot tell whether the orchestrator is live on its peer.

## What changes

When the child's changeset names an orchestrator on another daemon, the child's host asks that
peer whether it serves the orchestrator session's main terminal. That is the same rule a
same-host orchestrator is judged by.

- If the peer says yes, the hook waits as it would for a local orchestrator, and the forwarded
  answer wakes it.
- If the peer says no, or cannot be reached within a short bound, the child gets `NO_ORCHESTRATOR`
  and Claude's prompt shows, as today.

Asking never delays Claude's prompt by more than that bound.

## Non-goals

- Forwarding the wait itself: the hook stays held on the child's host.
- Any change to who may answer, or to the answer tools.
- Streaming RPC forwarding (`refuse_if_addressed_at_a_peer`'s TODO).

## Acceptance criteria

1. A child on host A whose orchestrator runs on host B waits. B's home agent's
   `AnswerSessionPermission`, forwarded to A, wakes the hook with B's answer.
2. The same holds for `AnswerSessionQuestion`.
3. If B does not serve the orchestrator's terminal, A answers `NO_ORCHESTRATOR` at once.
4. If B is unreachable, A answers `NO_ORCHESTRATOR` within the presence bound. The hook never waits
   on a dead peer.
5. A same-host orchestrator behaves exactly as before.
