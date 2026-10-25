# oversized-file: conversation_recorder.rs

**Location:** `packages/tddy-session-agents/src/conversation_recorder.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 by `/pr-wrap` step 3.5 file-length gate on PR #727 (`#coagents` 8/8)
**Metrics:** **502 production lines** · budget 500 · **1.004× over**
**Thresholds breached:** length 502 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-25 | 441 → 502 | first detection — PR #727 crossed it: `record_after_streamed` (an answered record written after the streamed frames it follows), `note_status` (the conversation's status row), the `ToolCallRecord` arm of `update_frame`, and the turn-error replies. The status row itself is its own module (`conversation_status.rs`). Not split: deferred to the `#coagents` sweep node by the developer (2026-10-25) |
| 2026-10-25 | 502 → 512 | **worse** by 10 in PR #776 (`#coagents` 9/9): the recorder holds the daemon's `AgentActivityHub`, so `note_status` publishes the status row live. Not split: the developer deferred every code issue of the `#coagents` stack past its sweep node too (2026-10-25), so this is left for after the stack lands |

## What would close it

`update_frame`, `tool_kind_of` and `tool_status_of` (the `AcpUpdate` → `AcpAgentMessage` mapping,
~85 lines) are a pure function group with no recorder state; moving them to a `conversation_frames.rs`
sibling takes the file under budget.

## Verified by hand

2026-10-25 — Counted to the first `#[cfg(test)]` at the rebased tip of PR #727 and at its base
(#726, `518110d17`).
