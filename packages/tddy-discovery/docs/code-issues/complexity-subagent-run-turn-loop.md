# complexity: run_turn_loop

**Location:** `packages/tddy-discovery/src/subagent.rs` — `SpecializedSubagentSession::run_turn_loop`
**Category:** complexity
**Detected:** 2026-09-26 by `/analyze-clean-code` during `/pr-wrap` on PR #545
**Metrics:** **70 lines raw · 43 code lines · control nesting depth 3**
**Thresholds breached:** length 70 > 60 (raw only — the code itself is tight)
**Restructure:** `extract_method` — ordinary work, and the smallest of the three in this PR
**Status:** Open — unclaimed

## Measurement history

| Run | Raw lines | Code lines | Nesting | Note |
|---|---|---|---|---|
| 2026-09-26 | 70 | 43 | 3 | first detection, in PR #545 |

## What the tool found

Over the ceiling on **raw** lines only: 43 of the 70 are code and the rest is justification prose,
which is the right trade for this particular body — it is where the turn budget, the total-outage
guard and the synthesis turn meet, and every one of those three has a documented reason to be
exactly where it is.

## What would close it

The total-outage block lifts cleanly to
`ToolCallTally::refuse_if_nothing_was_read(&self, model) -> Result<(), SubagentError>`, taking its
`log::warn!` with it. That is ~15 raw lines out and puts the guard on the type that owns the
tally.

## Neighbour worth measuring at the same time

`run_one_turn`, in the same file, is **exactly 60 lines at nesting depth 4** — sitting on both
thresholds without breaching either. Its tool-call loop extracts to
`async fn dispatch_and_record(&mut self, tool_calls, tally)`, taking it to ~45 and depth 3. Not a
finding today; it becomes one on the next line added.

## Verified by hand

2026-09-26 — Counted from the diff. Confirmed the outage block references only `self` and the
tally, so the extraction needs no new arguments.
