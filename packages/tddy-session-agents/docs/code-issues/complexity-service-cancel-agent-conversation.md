# complexity: cancel_agent_conversation

**Location:** `packages/tddy-session-agents/src/service.rs` — `SessionAgentServiceImpl::cancel_agent_conversation` (the `SessionAgentService` impl)
**Category:** complexity
**Detected:** 2026-10-25 by `/pr-wrap` on PR #727 (`#coagents` 8/8), hand-measured by brace matching
**Metrics:** **74 lines raw · brace depth 4**
**Thresholds breached:** length 74 > 60
**Restructure:** `extract_method` — ordinary work, no `/code-restructuring` needed
**Status:** Open — unclaimed

## Measurement history

| Run | Raw lines | Brace depth | Note |
|---|---|---|---|
| 2026-10-25 | 49 → 74 | 3 → 4 | first detection — PR #727 grew it: the cancelling actor is derived from the token (`AgentSessions::answerer`), and a local conversation's pending request is recorded as answered by that actor before the turn is interrupted (`record_cancelled_request`). Deferred to the `#coagents` sweep node by the developer (2026-10-25) |

## What the tool found

The local arm carries the whole cancel recording inline: record the held request, interrupt the
turn, tell the host the conversation closed, and map a failed record to an internal error that still
reports the cancel.

## Why it matters here

The order in that arm is the contract — record before interrupting, interrupt even when recording
failed. Inline, it reads as incidental sequencing.

## What would close it

- the `Local` arm → `async fn cancel_local(&self, conversation_id, closed, recorder, answering, cancelled_by) -> Result<(), Status>`
- the `Remote` arm → `async fn cancel_remote(&self, req, daemon_instance_id) -> Result<(), Status>`

What remains is about 30 lines: authenticate, name the actor, remove, note the activity, dispatch.

## Verified by hand

2026-10-25 — Counted at the rebased tip of PR #727 and at its base (#726, `518110d17`, 49 lines).
