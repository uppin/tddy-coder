# complexity: send_turn_and_check_final_answer

**Location:** `packages/tddy-discovery/src/subagent.rs` — `send_turn_and_check_final_answer`
**Category:** complexity
**Detected:** 2026-09-27 by hand measurement (brace-matched span, code lines = non-blank
non-comment) during the subagent-guardrails work that grew it
**Metrics:** **96 lines raw · 87 code lines**
**Thresholds breached:** length 96 > 60
**Restructure:** `extract_method` — ordinary work
**Status:** Open — unclaimed

## Measurement history

| Run | Raw lines | Code lines | Note |
|---|---|---|---|
| 2026-09-27 | 68 | — | before the subagent-guardrails work |
| 2026-09-27 | 96 | 87 | +28 for the generation cap and provider admission: `max_tokens`, the `ProviderQueue` acquire/release around `complete`, and the `MaxTokens` stop-reason branch |

**Not measured:** nesting, branches, CRAP. `/analyze-clean-code` was not run; these are
brace-matched hand measurements, so only the two figures above are stated. A later analyzer run
should add the rest rather than trust an absence here.

## What the tool found

Three concerns now share one body: build-and-send the request, hold the provider slot across it, and classify what came back. The provider acquire/release is the outlier — it is lifecycle, not turn logic, and it is the part that will grow again if model-affinity scheduling ever lands.

## What would close it

Lift the admission into a wrapper — `async fn with_provider_slot(&self, f)` — so the body sees a plain send. That is ~20 raw lines out and puts the slot's lifetime in one place instead of spanning the function.
