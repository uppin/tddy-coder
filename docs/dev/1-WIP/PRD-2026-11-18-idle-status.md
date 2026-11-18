# PRD: An idle session is reported as waiting for input, and a message to it is accepted

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Stack**: `#coder-start` 6/10 — branch `feature/coder-start/idle-status`, base `feature/coder-start/coder-transport`

## Problem

During the 2026-11-18 smoke test, a session idle at its prompt after a turn was reported with a
pending question of `kind: permission` and an empty `request_id`. `SendSessionMessage` refused it
(`waiting on permission ""`), `AnswerSessionPermission` had no id to answer, and the orchestrator
got through only by claiming the terminal and typing.

The cause is in the hook path: Claude's `Notification` hook with `idle_prompt` ("Claude is waiting
for your input") maps to `WaitingForInput` with no tool, and `report_session_status` then writes
`PendingQuestion { kind: "permission", question: <message> }` with no request id
(`tddy-session-activity/src/service/hook_reports.rs:140-149`). `SendSessionMessage` refuses on any
`pending-question.json`, and the idle-wake watch treats any pending question as not idle.

## Requirements

1. A `Notification` with no structured question behind it (`idle_prompt`, or any message with no
   tool) records the session as **waiting for input** and writes **no** pending-question record.
2. A pending-question record is only ever written with a request id that `AnswerSessionPermission` /
   `AnswerSessionQuestion` can answer.
3. `SendSessionMessage` is refused only while an **answerable** question is pending.
4. A session waiting for input with nothing to answer counts as idle for idle wakes.
5. Structured questions (PermissionRequest, AskUserQuestion) and the Notification that follows them
   behave as today.

## Acceptance criteria

- [ ] An `idle_prompt` Notification leaves the session `WaitingForInput` with no `pending-question.json`
- [ ] `SendSessionMessage` to that session is delivered
- [ ] An idle wake fires for that session
- [ ] A PermissionRequest followed by its Notification still keeps the structured question with its id
- [ ] Scoped gates: `./test -p tddy-session-activity -p tddy-daemon-rpc`, clippy `-D warnings`

## Out of scope

- The same session's `ReadSessionTerminal` "has no live terminal" while executing a tool, and
  `ReadSessionActivity` returning nothing from cursor 0 (seen once, not reproduced; separate reports).
