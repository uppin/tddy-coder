# complexity: run_one_turn

**Location:** `packages/tddy-discovery/src/subagent.rs` — `run_one_turn`
**Category:** complexity
**Detected:** 2026-09-27 by hand measurement (brace-matched span, code lines = non-blank
non-comment) during the subagent-guardrails work that grew it
**Metrics:** **64 lines raw · 60 code lines**
**Thresholds breached:** length 64 > 60
**Restructure:** `extract_method` — ordinary work
**Status:** Open — unclaimed

## Measurement history

| Run | Raw lines | Code lines | Note |
|---|---|---|---|
| 2026-09-27 | 60 | — | before the subagent-guardrails work |
| 2026-09-27 | 64 | 60 | +4 for recording each dispatch outcome into the repeat ledger |

**Not measured:** nesting, branches, CRAP. `/analyze-clean-code` was not run; these are
brace-matched hand measurements, so only the two figures above are stated. A later analyzer run
should add the rest rather than trust an absence here.

## What the tool found

Four raw lines over, and 60 of the 64 are code — there is almost no prose to trim. It is the per-turn loop body: send, dispatch each call, tally, append.

## What would close it

Marginal. The tally and ledger bookkeeping could move behind one `note(dispatch)` call on a type that owns both, which is ~6 lines out and also removes the risk of updating one and forgetting the other.
