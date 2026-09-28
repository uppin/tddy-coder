# oversized-file: conversation.rs

**Location:** `packages/tddy-discovery/src/roster/conversation.rs`
**Category:** oversized-file
**Detected:** 2026-09-28 by `/pr-wrap` step 3.5 file-length gate on PR #556 (`#subagent-control` 4/5)
**Metrics:** **512 production lines** · budget 500 · **1.02× over**
**Thresholds breached:** length 512 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-28 | 488 | first detection — under budget before PR #556; the yield-condition parse framing (`parse_message_descriptor`'s stop-reason family + the final-chunk condition echo) crossed it |
| 2026-09-28 | 512 | +24 in PR #556 — decomposition **deferred by explicit developer consent**: node 5 (`resume-replacement`, #557) threads the same framing surfaces and a split now would cascade through its diff; the split gets its own change after the `subagent-control` stack lands |

## What would close it

The framing concern — `parse_message_descriptor` and the final-chunk condition echo — is the
cohesive group; the conversation loop is the rest. Engine-driven extraction
(`code-restructuring`) after the stack lands, with `parse_message_descriptor`'s callers repointed.
