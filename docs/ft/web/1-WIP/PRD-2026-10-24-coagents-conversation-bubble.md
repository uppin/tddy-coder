# A subagent conversation is one activity bubble that opens the conversation - PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement
**Stack**: `#coagents` 4/8, branch `feature/coagents/conversation-bubble`, base `feature/coagents/permission-yield` (#722)

## Affected Features

- **Primary Feature**: [Agent activity pane](../agent-activity-pane.md) — a subagent conversation is one bubble; clicking it opens the conversation
- **Related Feature**: [ACP replay — lazy tool bodies](../../coder/acp-replay-lazy-tool-bodies.md) — the replay RPCs gain a conversation axis
- **Related Feature**: [Session agent roster](../../daemon/session-agent-roster.md) — a daemon-run conversation's turns are recorded and observable

## Summary

Today the main agent's use of a subagent shows up in the activity pane as a scatter of one-line tool
bubbles — `mcp__tddy-tools__subagent_new_session`, `…_prompt`, `…_await`, `…_await`, `…_resume` —
and what the subagent itself did is not visible at all. This PRD makes **each subagent conversation
one bubble** in the session's activity pane, showing the agent, what it was asked and its live
status. Clicking it opens **the conversation itself**: the subagent's own ACP transcript — its text,
its tool calls with their detail, the permission requests it raised — rendered with the same
read-only transcript and detail dialog the activity pane already uses, live while the conversation
runs.

## Background

The web's read path is built around one session directory: `StreamAcpReplay(session_id)` reads that
session's `acp-transcript.jsonl` and `agent-activity.jsonl`. A daemon-run subagent conversation has
no transcript at all — its answers travel back to the caller once and are gone
([`docs/dev/todo/2026-08-29-a-roster-agent-s-own-turns-are-unobservable-from-the-web.md`](../../../dev/todo/2026-08-29-a-roster-agent-s-own-turns-are-unobservable-from-the-web.md)).

## Proposed Changes

### What's Changing

- **Daemon-run conversations keep a transcript.** Every conversation the daemon runs — coding
  agents, and `model` agents served by the daemon — records its prompts, its streamed text, its tool
  calls and its permission requests as ACP frames under the session it belongs to, one transcript per
  conversation.
- **The replay RPCs take a conversation.** `StreamAcpReplay`, `GetAcpReplayPage` and
  `GetAcpToolCallDetail` accept an optional `conversation_id`; with it they read that conversation's
  transcript, with the same modes, paging and lazy bodies.
- **The session's own transcript gets one entry per conversation.** The main agent's
  `subagent_*` calls for one conversation collapse into **one bubble** keyed by the conversation:
  agent label, harness or model, the first prompt's summary, and a status that follows the
  conversation — running, waiting for permission, idle, ended, failed.
- **Clicking the bubble opens the conversation** in a dialog: the read-only transcript (the same
  component as the activity pane), live while it runs, older turns paged in, and each of its tool
  calls clickable for its detail exactly as in the pane.
- A pending permission request in the conversation is shown in the transcript as an entry naming the
  tool and the options (answering it from the web is the next node).

### What's Staying the Same

- Every other activity bubble, its detail dialog and the pane's open/close behaviour.
- Conversations run **in the jail** (seeded `model` agents) are still unrecorded — their history is
  in the jail's memory only. Their `subagent_*` calls still collapse into one bubble, but clicking it
  says the conversation was not recorded.
- The Agents tab conversation tabs (the operator's own conversations) are unchanged.

## Impact Analysis

### Technical Impact

- `tddy-service`: `conversation_id` on three replay requests; transcript path helpers.
- `tddy-session-agents`: appends frames per daemon-run conversation turn.
- `tddy-session-activity`: serves a conversation's transcript; folds `subagent_*` hook records into a
  per-conversation entry.
- `tddy-web`: `ChatMessage` kind `subagent`; a `SubagentConversationDialog` hosting `useAcpReplay`
  keyed by conversation.

### User Impact

- The activity pane says what each subagent is doing; the conversation is one click away.

## Acceptance Criteria

- [ ] A daemon-run conversation's prompt, text, tool calls and permission requests are replayable by `(session_id, conversation_id)`.
- [ ] Paging and tool-call detail work for a conversation as for a session.
- [ ] In the session's activity, one conversation's `subagent_*` calls are one bubble, with the agent's label and a status that follows the conversation.
- [ ] Clicking the bubble opens the conversation's transcript, live while it runs.
- [ ] A tool call inside the conversation opens its detail.
- [ ] A pending permission request shows as an entry naming the tool and options.
- [ ] An unrecorded (in-jail) conversation's bubble opens a dialog saying it was not recorded.

## References

- [Agent activity pane](../agent-activity-pane.md), [ACP replay — lazy tool bodies](../../coder/acp-replay-lazy-tool-bodies.md)
- Stack parents: #720, #721, #722
