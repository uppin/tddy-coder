# complexity: dispatch_tool_call

**Location:** `packages/tddy-discovery/src/subagent.rs` — `dispatch_tool_call`
**Category:** complexity
**Detected:** 2026-09-27 by hand measurement (brace-matched span, code lines = non-blank
non-comment) during the subagent-guardrails work that grew it
**Metrics:** **74 lines raw · 65 code lines**
**Thresholds breached:** length 74 > 60
**Restructure:** `extract_method` — ordinary work
**Status:** Open — unclaimed

## Measurement history

| Run | Raw lines | Code lines | Note |
|---|---|---|---|
| 2026-09-27 | 60 | — | before the subagent-guardrails work |
| 2026-09-27 | 74 | 65 | +14 for argument validation, which runs before the dispatch match |

**Not measured:** nesting, branches, CRAP. `/analyze-clean-code` was not run; these are
brace-matched hand measurements, so only the two figures above are stated. A later analyzer run
should add the rest rather than trust an absence here.

## What the tool found

A long `match` over ten tool names, now preceded by a validation gate. The match arms are flat and each is two or three lines — the length is breadth, not depth, and breaking it up by tool would scatter the dispatch table nobody currently has to hunt for.

## What would close it

Lift the validation gate into the caller (`dispatch_bounded`, which already gates on tool binding and repeats) so all three refusals sit together and this function is purely the dispatch table. ~12 raw lines out, and it groups the three reasons a call never runs.
