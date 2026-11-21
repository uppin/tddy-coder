# oversized-file: remote_hook_follower.rs

**Location:** `packages/tddy-agent-launch/src/remote_hook_follower.rs`
**Category:** oversized-file
**Detected:** 2026-11-21 — `/pr-wrap` step 3.5 on `#target-parity` 1/7 (#969), production lines counted to the first `#[cfg(test)]`
**Metrics:** **511 production lines** · budget 500
**Thresholds breached:** length 511 > 500
**Restructure:** `move_item` — `/code-restructuring` territory
**Status:** Open — unclaimed; **deferred to the `#target-parity` sweep node**: the file is stack-shared (n4 #972 and n6 #974 also modify it), and `/yolo-pr-stack` § Stage W forbids splitting a stack-shared file inside a node
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-11-21 | 444 → 511 | first detection; **+67** in #969: `HookLine::answer_nonce` and its parsing, `HookReportDelivery::await_question_answer` and the `request_id` return, `ActivityHookSink::answering_through` and `answer_in_background`, the answers directory in the git exclude and in `remove_remote_hook_log`. The hold and the answer write themselves went to a new module, `remote_hook_answer.rs` (110). This PR pushed the file past 500 |

## What would close it

Move the delivery side into a module of its own: one `move_item` over
`tddy_agent_launch::remote_hook_follower::{HookReportDelivery, ActivityHookSink, <ActivityHookSink>,
<ActivityHookSink as HookSink>}` (contiguous, lines 128–256 on `#969`'s head) with
`"name": "remote_hook_delivery"`, `"to": "tddy_agent_launch"`, re-exported from
`remote_hook_follower` so `HookReportDelivery` keeps its path for `tddy-session-lifecycle` and
`tddy-session-driver`. That brings the file to about 380 lines. Do it once every `#target-parity`
node that edits this file has landed on the branch the sweep starts from (n4 and n6 add the
`PreToolUse` and question paths to the same sink).
