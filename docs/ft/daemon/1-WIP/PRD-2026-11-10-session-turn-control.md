# Orchestrators drive a session's turns — PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#orchestrators` 4/11 (`feature/orchestrators/session-turn-control`)

## Affected Features

- **Primary**: [Project concept § Project-home tools](../project-concept.md#project-home-tools) — two
  tools, and a turn-finished record the notification tools report
- [Project concept § Waiting for notifications](../project-concept.md#waiting-for-notifications)
- [Session notifications](../session-notifications.md) — project notification records name the orchestrator

## Summary

An orchestrator (main today; any orchestrator-role session once `#orchestrators` 1/11 lands) can talk
to a session the way `/yolo-pr-stack` talks to a subagent: **send it a follow-up turn**, **resume it**
when it has stopped (optionally with a message), and **be told when its turn ends**.

- `SendSessionMessage { session_id, text, daemon_instance_id? }` → `{ session_id, delivered_via }`.
  How a turn reaches a running agent is declared in the agent's YAML spec (`turn_input`): a terminal
  paste followed by the submit key for a terminal agent, the ACP prompt for an ACP agent. A session
  whose agent declares none is refused naming the agent. A stopped session is refused naming
  `ResumeSession`. A session waiting on a permission prompt or a question is refused naming the
  answer tools.
- `ResumeSession { session_id, message?, daemon_instance_id? }` → `{ session_id, outcome: resumed |
  running }`: what the web's Resume does, then the message as its first turn.
- **Told when a turn ends**: the project notification log already records an `attention_required`
  record when a session finishes a turn and waits. Each record now also names the session's
  orchestrator (`orchestrator_session_id`), and `WaitProjectNotifications` / `ReadProjectNotifications`
  take `started_by_me: true` to return only the caller's own sessions — so an orchestrator waits on its
  children's turns without reading everyone else's.

## Proposed Changes

### What's Changing
- Two project-home tools, forwarded across hosts like the others.
- Agent spec slot `turn_input`; claude's declares terminal paste + submit.
- `orchestrator_session_id` on notification records; `started_by_me` on the two notification tools.

### What's Staying the Same
- `SendSessionKeys` and its lease (`SendSessionMessage` takes no lease and does not disturb an attached
  browser's control); the answer tools; target rules (same project, not the caller).

## Acceptance Criteria
- [ ] Main sends a message to a running claude-cli session; it is taken as a new turn
- [ ] A message to a stopped session is refused naming `ResumeSession`; `ResumeSession` with a message resumes it and delivers it
- [ ] A session waiting on a prompt is refused naming the answer tools
- [ ] When a session main started ends its turn, `WaitProjectNotifications { started_by_me: true }` returns that record and no other session's
- [ ] Both tools work against a session on another host

## References
- [Project concept](../project-concept.md), [Session notifications](../session-notifications.md)
