# Project-home agent answers a permission prompt - PRD

**Date**: 2026-10-25
**PRD Type**: Enhancement
**Stack**: `#home-inbox 6/8` (branch `feature/home-inbox/permission-answer`)

## Affected Features

- **Primary**: [Claude Code CLI session](../claude-cli-session.md) - an orchestrated session's `PermissionRequest` hook waits, for a bounded time, for its orchestrator's decision
- **Related**: [Project concept § Project-home tools](../project-concept.md) - `AnswerSessionPermission`

## Summary

When a child that was started by an **active** project-home session asks for permission:

1. Its `PermissionRequest` hook waits up to `answer_wait_secs` (default 120) for the home
   agent's answer.
2. The home agent answers with
   `AnswerSessionPermission(session_id, daemon_instance_id?, request_id, option_id | cancelled, message?)`.
3. Claude proceeds as if a person had answered.
4. If no answer arrives in time, or the child has no active orchestrator, the hook returns without a
   decision and Claude shows its own prompt, exactly as today.

## Background

`#home-inbox 3/8` tells the home agent what a child is asking. Answering through keystrokes
(`#home-inbox 5/8`) works, but it depends on the screen layout. Claude's `PermissionRequest` hook can
return a decision. The daemon has no "hold a caller until somebody answers" RPC today.

## Proposed Changes

### What's Changing

- **The waiting hook.** In an orchestrated claude-cli session (a changeset `orchestrator_session_id`
  whose home session is running), `PermissionRequest` runs `tddy-tools session-hook --await-answer`.
  - It reports the pending question, then calls `ActivityService.AwaitQuestionAnswer`, which holds
    until an answer or the deadline.
  - On an answer it prints Claude's decision JSON:
    `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"|"deny","message"}}}`.
  - It never prints anything else.
- **Bounds.**
  - Claude's hook `timeout` for that entry is `answer_wait_secs + 10`.
  - On a daemon error, the deadline, or no orchestrator, the hook prints nothing and Claude falls
    back to its own prompt.
- **`AnswerSessionPermission`** (project-home tool) checks, in order:
  1. the target is in the home agent's project and was started by **this** home session;
  2. the pending question is still pending;
  3. it is a **permission** (a question is refused, naming `AnswerSessionQuestion`).

  It then wakes the waiting hook. A pending question answered twice is refused the second time, naming
  who answered first.
- **Recording.** The decision is recorded on the pending question (who, when, what) and the pending question is
  cleared.

### What's Staying the Same

- Sessions with no orchestrator, or whose orchestrator isn't running, behave exactly as today.
- A person can still answer in Claude's terminal after the wait ends.
- Questions are `#home-inbox 8/8`.

## Impact Analysis

- **Technical.**
  - A pending-answer registry in the daemon (in memory, keyed by `request_id`).
  - One long-poll RPC.
  - A hook mode that prints a decision.
  - One project-home tool.
- **User.** A child stalls for at most two minutes on a decision its orchestrator can make.
- **Safety.**
  - Only the home session that started a child may answer for it.
  - The wait is bounded.
  - Every decision is logged in both sessions.

## Acceptance Criteria

- [ ] An orchestrated child's permission hook, answered `allow` by its home agent, prints the allow
  decision and exits within the budget.
- [ ] `deny` with a message prints the deny decision with that message.
- [ ] With no answer before the deadline, the hook prints nothing and exits 0, so Claude's own
  prompt follows.
- [ ] A child without an active orchestrator gets no wait. The hook returns immediately.
- [ ] Another home session, a non-project-home caller, an unknown id and an already-answered
  pending question are each refused with a reason, and the hook stays waiting.

## Out of scope

- Answering questions (`#home-inbox 8/8`).
- Answering from Telegram or the web.
- Policies that auto-answer.

## Changes from the 2026-10-25 plan

Replan of 2026-10-26: `master` moved, and this stack now sits on `#start-parity 9/9` (#758).

- **Answer shape.** The answer uses `#coagents`' vocabulary. The request is offered as a
  `PermissionRequestView`, with options `allow_once` and `reject_once`, plus `allow_always` when Claude
  sends permission suggestions. An answer names an `option_id` or is `cancelled`. The fields mirror
  `AnswerAgentPermission` (`session_id`, `daemon_instance_id`, `request_id`, `option_id`, `cancelled`).
  A `reject_*` option prints `deny` with the message, and `cancelled` prints nothing.
- **Who may answer.** The child's changeset `orchestrator_session_id` must name the caller. That covers
  a project-home session and a spawn-capable session (`#start-parity 9/9`) alike, not only the
  project-home case.
- **Hosts.** The tool is forwarded to the child's host like every other target tool. The hook waits
  only when the orchestrator runs on that same host; an orchestrator on another host gets no wait in
  this node.
- **The record is `#start-parity 6/9`'s pending question.** `3/8` extends it, and `request_id`
  replaces the old `elicitation_id`.
