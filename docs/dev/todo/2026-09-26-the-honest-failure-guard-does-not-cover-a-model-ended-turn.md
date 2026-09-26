# 2026-09-26 — The tool-outage guard does not cover a turn the model ends

**Category:** Known limitation
**Source:** `2026-09-26-subagent-turn-control-and-honest-tool-failure` changeset, PR #545 —
`/validate-changes` finding I1

PR #545's headline behaviour is that a prompt in which **no tool call succeeded** returns an error
instead of a fabricated summary. The guard sits at budget exhaustion, immediately before
`run_synthesis_turn`.

`run_turn_loop` returns early whenever a turn yields an outcome, **without consulting the tally**.
So a model that emits a confident `<final_answer>` after N refused tool calls still answers, and
the caller still gets a plausible summary of nothing. The 2026-09-26 incident happened to exhaust
its budget, which is why the guard as placed catches it.

`ToolCallTally::total_outage`'s own doc is honest about this ("a prompt the **model** ended never
reaches here"), but **PRD AC6 as written is broader than what ships**, and the changeset's Summary
says "a prompt in which no tool call succeeded is an error" without the qualifier.

## Why it was left

Extending the guard to model-ended turns was tried during green and reverted. It made
`a_failed_tool_result_is_marked_and_its_error_text_survives_quoting` and
`a_conversation_whose_prompt_failed_can_still_be_rewound_into` irreconcilable without inspecting
the *text* of the tool error to decide which failures "count" — cross-crate prose matching, which
is the coupling the typed `ToolDispatchOutcome` exists to abolish. The test that forced it turned
out to be over-specified and was fixed, but the underlying tension is real: **a single failed
`READ` on a missing file must not turn a good answer into an error.**

## What closing it would take

A rule that distinguishes "the model answered having read something" from "the model answered
having read nothing", without reading error strings. The material now exists that did not before:
`ToolDispatchOutcome` carries the transport/tool distinction at the jail boundary, and
`ToolDispatch` carries `Ran`/`NeverRan` inside the subagent. Propagating the jail's distinction
into the subagent's dispatch envelope — the `TODO(AC3)` that was removed as unnecessary at the
time — would let the guard fire on *transport* failures regardless of how the turn ended, and stay
quiet for a missing file.
