# oversized-file: remote_hook_follower.rs

**Location:** `packages/tddy-agent-launch/src/remote_hook_follower.rs`
**Category:** oversized-file
**Detected:** 2026-11-21 — `/pr-wrap` step 3.5 on `#target-parity` 1/7 (#969), production lines counted to the first `#[cfg(test)]`
**Metrics:** **526 production lines** (2026-11-21, `#target-parity` sweep; 517 at #974) · budget 500
**Thresholds breached:** length 526 > 500
**Restructure:** `move_item` — `/code-restructuring` territory
**Status:** Open — **deferred until the engine is fixed** (developer decision, 2026-11-21, `#target-parity` sweep): the `move_item` below is refused by `restructure check --deep`, and no hand move is consented. Engine defect: `docs/dev/todo/2026-11-21-restructure-engine-refuses-the-target-parity-splits.md`
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-11-21 | 444 → 511 | first detection; **+67** in #969: `HookLine::answer_nonce` and its parsing, `HookReportDelivery::await_question_answer` and the `request_id` return, `ActivityHookSink::answering_through` and `answer_in_background`, the answers directory in the git exclude and in `remove_remote_hook_log`. The hold and the answer write themselves went to a new module, `remote_hook_answer.rs` (110). This PR pushed the file past 500 |
| 2026-11-21 | 511 → 516 | **+5** in #972 (`#target-parity` 4/7): the `reports_a_status` gate in `ActivityHookSink::deliver`; the rule itself went to a new module, `remote_hook_report.rs` (13). Not split: stack-shared |
| 2026-11-21 | 516 → 517 | **+1** in #974 (`#target-parity` 6/7): `question_input` on the held hook; its rule, `HookLine::question_input`, lives in `remote_hook_answer.rs`. Not split: stack-shared |

| 2026-11-21 | 517 → 526 | **+9** in the `#target-parity` sweep: `ActivityHookSink::answering_through` takes the session's `answer_wait_secs` and the held hook carries the end of its window, so a late answer is removed from the target (the rule itself is in `remote_hook_answer.rs`). **Split attempted and refused by the engine** — see below |

## Engine refusal (2026-11-21)

The `move_item` named below, written as a v2 plan (`reexport: outside`, and again `named`), with the
warm index daemon (`TDDY_INDEX_SOCKET` set):

```text
$ ./dev target/debug/tddy-tools restructure check tmp/restructure/follower.jsonl --deep
0: plan is malformed: `mod tests;` leads to neither `tests.rs` nor `tests/mod.rs` in packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs
Error: 1 finding(s) — see above. Nothing was written.
```

The engine's module walk ignores `#[path = "restore_tabs_tests.rs"]` on that declaration
(`src/opencode_cli_spawn/restore_tabs.rs:26`), a file the move does not touch, so every `move_item` in
this crate is refused until it is fixed.

## What would close it

Move the delivery side into a module of its own: one `move_item` over
`tddy_agent_launch::remote_hook_follower::{HookReportDelivery, ActivityHookSink, <ActivityHookSink>,
<ActivityHookSink as HookSink>}` (contiguous, lines 128–256 on `#969`'s head) with
`"name": "remote_hook_delivery"`, `"to": "tddy_agent_launch"`, re-exported from
`remote_hook_follower` so `HookReportDelivery` keeps its path for `tddy-session-lifecycle` and
`tddy-session-driver`. That brings the file to about 380 lines. Do it once every `#target-parity`
node that edits this file has landed on the branch the sweep starts from (n4 and n6 add the
`PreToolUse` and question paths to the same sink).
