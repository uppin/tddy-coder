# oversized-file: session.rs

**Location:** `packages/tddy-coding-subagent/src/session.rs`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` step 3.5 file-length gate on PR #722 (`#coagents` 3/8)
**Metrics:** **530 production lines** · budget 500 · **1.06× over**
**Thresholds breached:** length 501 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-24 | 323 → 501 | first detection — PR #722 crossed it: the in-flight prompt held across tddy turns (`InFlightPrompt`, `settle`, `resume`, `finish`) and the cancel that drives a paused prompt to its end. Not split: stack-shared — later `#coagents` nodes modify this file, so a decomposition here would conflict through their diffs |
| 2026-10-24 | gate: 501 → 530 | **worse** by 29 in PR #723 (`#coagents` 4/8): the update broadcast (`update_stream`, `UPDATE_STREAM_CAPACITY`, `SubagentSession::updates`) and the recorder sink (`record_updates_with`) wired into `begin_prompt`'s one fan-out. Not split: stack-shared — after the stack lands |
| 2026-10-25 | gate: 530 → 575 | **worse** by 45 in PR #726 (`#coagents` 7/8): the session holds a `dyn ConversationDriver` — `start_headless`, `with_conversation` (the shared constructor both starts use) and `permissions()`, the handle the approval route parks on. Not split: stack-shared; deferred to the `#coagents` sweep node by the developer (2026-10-25) |

## What would close it

The cancel path — `CancelHandle`, `CANCEL_SETTLE_DEADLINE` and `drive_cancelled_turn_to_its_end` —
is one cohesive group (~70 lines) that only shares `InFlightPrompt` with the rest; moving it to a
`cancel.rs` sibling (engine-driven, `code-restructuring`) after the `#coagents` stack lands takes the
file under budget.
| 2026-10-25 | gate: 575 → 699 | **worse** by 124 in PR #727 (`#coagents` 8/8): the main agent's answer delivered before the in-flight slot is taken (`deliver_main_agent_answer`), the deferral wait in `take_turn`/`resume`/`settle`, the streamed permission records and `permission_answerer`. The records and the user answers have their own modules (`permission_records.rs`, `user_permission_answers.rs`). Not split: deferred to the `#coagents` sweep node by the developer (2026-10-25) |
