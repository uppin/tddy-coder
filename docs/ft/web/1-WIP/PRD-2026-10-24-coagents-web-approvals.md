# The main agent can hand a subagent's permission request to the user - PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement
**Stack**: `#coagents` 8/8, branch `feature/coagents/web-approvals`, base `feature/coagents/headless-driver` (#726)

## Affected Features

- **Primary Feature**: [Agent activity pane](../agent-activity-pane.md) — the subagent conversation view answers a permission request handed to the user
- **Related Feature**: [Managed-codebase subagents](../../coder/managed-codebase-subagents.md) — `subagent_resume { permission: { requestId, deferToUser: true } }`
- **Related Feature**: [Session agent roster](../../daemon/session-agent-roster.md) — `AnswerAgentPermission`

## Summary

A coding subagent's permission request goes to the main agent, which decides. This PRD gives the main
agent a third answer besides choosing an option or cancelling: **hand it to the user**. The request
then shows in the subagent's conversation view — reached from its activity bubble — with the agent's
options as buttons; the user's choice is delivered to the subagent, and the main agent, waiting on
`subagent_await`, gets the rest of the turn.

## Background

The user chose that approvals are the invoking agent's to handle, and that it may escalate. The main
agent's own way of asking the user (a chat question) does not show what the subagent is about to do;
the conversation view does — the tool call, its input, the transcript leading up to it.

## Proposed Changes

### What's Changing

- **Defer to the user.** `subagent_resume { sessionId, permission: { requestId, deferToUser: true } }`
  leaves the request open for the user and returns `{ responseId, pending: true, awaitingUser: true }`.
  `subagent_await { responseId }` returns the continuation once the user answers — or
  `{ pending: true, awaitingUser: true }` while they have not.
- **The bubble says so.** The subagent's activity bubble shows "waiting for you" while a request is
  deferred to the user.
- **Answer in the conversation view.** The pending request entry shows the tool call (title, kind,
  input on demand) and one button per option; choosing one sends `AnswerAgentPermission` and the
  entry records the choice and who made it. A request the main agent did not defer shows no buttons —
  it is the main agent's to answer.
- **Who may answer.** The session's owner (the same authorisation as prompting the session); anyone
  else is refused.
- **Withdrawal.** The main agent may still answer a deferred request itself (`subagent_resume` with an
  option) — the first answer wins, and the other gets a "already answered by …" refusal.

### What's Staying the Same

- Requests the main agent answers itself.
- The conversation view for every other entry.

## Impact Analysis

### Technical Impact

- Proto: `SessionAgentService.AnswerAgentPermission`, relayable from the jail (`IN_JAIL_RELAYABLE` is
  not needed — the web calls it, not the jail) and peer-forwarded.
- `tddy-coding-subagent`: a parked request can be deferred and answered by either side once.
- `tddy-tools`: `deferToUser`, `awaitingUser`.
- `tddy-web`: answer buttons on the pending entry; bubble status.

### User Impact

- When the main agent is unsure, the user sees exactly what the subagent wants to do and decides.

## Acceptance Criteria

- [ ] `deferToUser` returns a pending response marked `awaitingUser`.
- [ ] The bubble shows "waiting for you".
- [ ] The conversation view shows the request with one button per option, only when deferred.
- [ ] Choosing an option delivers it to the subagent; `subagent_await` returns the continuation.
- [ ] The entry records the choice and who made it.
- [ ] A non-owner is refused.
- [ ] The first answer wins; the second is refused naming who answered.
- [ ] It works when the conversation is served by another daemon.

## References

- [Agent activity pane](../agent-activity-pane.md), [Managed-codebase subagents](../../coder/managed-codebase-subagents.md)
- Stack parents: #720–#726
