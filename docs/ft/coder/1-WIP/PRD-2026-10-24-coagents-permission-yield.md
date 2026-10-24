# A coding subagent's permission request is the main agent's to answer - PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement
**Stack**: `#coagents` 3/8, branch `feature/coagents/permission-yield`, base `feature/coagents/acp-subagent` (#721)

## Affected Features

- **Primary Feature**: [Managed-codebase subagents](../managed-codebase-subagents.md) — a new stop reason, `permission_requested`, and a `permission` answer on `subagent_resume`
- **Related Feature**: [Session agent roster](../../daemon/session-agent-roster.md) — the conversation RPCs carry the request and the answer across daemons

## Summary

When a coding subagent asks for permission — to run a command, write a file, use a tool its policy
does not pre-approve — the request goes to **the agent that invoked it**. The turn yields back to the
main agent with the request and its options; the main agent answers it with `subagent_resume`, and
the subagent carries on from where it stopped. The main agent may decide on its own or ask the user
first; that choice is the main agent's, not the subagent's and not tddy's.

## Background

The previous node denies every permission request so a subagent never acts unapproved on the main
agent's worktree. That is safe and useless for real work: Claude Code, Codex and OpenCode all ask
before edits and commands under their default policies. ACP models the ask as
`session/request_permission` with a tool call and a list of options (allow once, allow always,
reject once, reject always). The subagent turn already has a yield-to-caller shape
(`stopReason: "yielded_to_caller"`, answered by `subagent_resume`); a permission request is the
natural second reason to yield.

## Proposed Changes

### What's Changing

- **The turn yields on a permission request.** `subagent_prompt` / `subagent_await` /
  `subagent_resume` can return `stopReason: "permission_requested"` with a `permissionRequest`:
  `{ requestId, toolCall: { title, kind, rawInput }, options: [{ optionId, name, kind }] }`. The
  subagent is paused, waiting — its turn is not over.
- **The main agent answers with `subagent_resume`.** `subagent_resume { sessionId, permission: {
  requestId, optionId } }` delivers the chosen option, and the call returns the rest of the turn —
  the same outcome shape, which may be another `permission_requested`. `permission: { requestId,
  cancelled: true }` answers "cancelled".
- **Long waits behave like long turns.** The answer's continuation obeys the grace period and
  `responseId` / `subagent_await` exactly as a prompt does.
- **One question at a time.** While a request is pending, `subagent_prompt` on that conversation is
  refused naming the pending `requestId`; `subagent_resume` without a `permission`, or with a
  `requestId` that is not the pending one, is refused naming it.
- **Cancelling while one is pending** answers the request as cancelled, cancels the turn, and returns
  `stopReason: "cancelled"`.
- **The tool descriptions say who decides.** `subagent_resume`'s description tells the main agent
  that the decision is its own and that it may consult the user before answering.
- **Across daemons.** The request and the answer travel on the existing conversation RPCs, so the
  same works when the conversation is served by a daemon other than the one the main agent talks to.

### What's Staying the Same

- `model` (OpenAI-loop) agents never produce `permission_requested`; their `yieldConditions` /
  `yielded_to_caller` path is unchanged.
- The workflow ACP backends keep allowing once; this only changes subagent conversations.
- `subagent_resume`'s other options stay refused for coding agents (previous node).

## Impact Analysis

### Technical Impact

- `tddy-discovery`: `StopReason::PermissionRequested`, `PromptOutcome.permission_request`,
  `TurnRequest.permission_answer`.
- `tddy-coding-subagent`: a `PermissionHandler` that parks the request and resolves it from the
  resume.
- `tddy-tools`: `subagent_resume` gains `permission`; outcome JSON gains `permissionRequest`.
- Proto: `AgentConversationChunk.permission_request_json`, `ResumeAgentConversationRequest.permission_answer_json`;
  both stop-reason spelling tables gain `permission_requested`.

### User Impact

- A main agent can delegate real edits to a coding subagent and keep the approval decisions — or hand
  them to the user through its own conversation.

## Acceptance Criteria

- [ ] A subagent permission request ends the call with `permission_requested` and the request's tool call and options.
- [ ] `subagent_resume` with an option continues the same subagent turn, and the agent receives that option.
- [ ] A cancelled answer reaches the agent as cancelled.
- [ ] A continuation longer than the grace period returns a `responseId` and is awaitable.
- [ ] A prompt while a request is pending is refused naming the request.
- [ ] A resume naming another request id, or with no permission, is refused naming the pending one.
- [ ] `subagent_cancel` while pending returns `cancelled` and the agent sees the request cancelled.
- [ ] The same holds when the conversation is served by another daemon in the session room.

## References

- [Managed-codebase subagents](../managed-codebase-subagents.md) § `subagent_resume`
- Stack parents: #720, #721
