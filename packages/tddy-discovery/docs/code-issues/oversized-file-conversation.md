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
| 2026-09-28 | 522 | +10 more in PR #557 (`#subagent-control` 5/5): the replacement fields' framing in the same parse — inside the standing deferral (the stack's top node; split after landing) |
| 2026-10-01 | 526 | **worse** by 4 over master's 522 in PR #560 (`#agent-worktree` 1/4): the `worktree_change: None` on a daemon-run turn's descriptor and its `TODO(agent-worktree)` comment. Inside the standing deferral (split after the stack lands) |
| 2026-10-24 | 526 → 551 | **worse** by 25 in PR #722 (`#coagents` 3/8): the final frame's `permission_request_json` parse, `encode_permission_answer` on the resume request, and the `PermissionRequested` wire spelling. Not split: the file is stack-shared — later `#coagents` nodes modify it, so a decomposition here would conflict through their diffs. Split after the stack lands, along the framing seam this record names |
| 2026-10-25 | 551 → 560 | **worse** by 9 in PR #727 (`#coagents` 8/8): the resume request carries `defer_permission_to_user`, and `RemoteConversationHandle::take_turn` runs a turn without the session a held turn keeps. Not split: deferred to the `#coagents` sweep node by the developer (2026-10-25) |
