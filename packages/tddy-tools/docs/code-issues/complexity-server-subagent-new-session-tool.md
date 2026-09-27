# complexity: subagent_new_session_tool

**Location:** `packages/tddy-tools/src/server.rs` — `subagent_new_session_tool`
**Category:** complexity
**Detected:** 2026-09-27 by hand measurement (brace-matched span, code lines = non-blank
non-comment) during the subagent-guardrails work that grew it
**Metrics:** **96 lines raw · 76 code lines**
**Thresholds breached:** length 96 > 60
**Restructure:** `extract_method` — ordinary work
**Status:** Open — unclaimed

## Measurement history

| Run | Raw lines | Code lines | Note |
|---|---|---|---|
| 2026-09-27 | 61 | — | before the subagent-guardrails work |
| 2026-09-27 | 96 | 76 | +35 for the `systemPrompt` override: reading it, refusing one aimed at a remotely-routed agent, and threading it into the config on the local path |

**Not measured:** nesting, branches, CRAP. `/analyze-clean-code` was not run; these are
brace-matched hand measurements, so only the two figures above are stated. A later analyzer run
should add the rest rather than trust an absence here.

## What the tool found

The body now carries two whole paths — open a conversation on a def this process holds, or open one on the facilitating daemon — and the override applies to only one of them, so the refusal for the other is inline. The remote and local halves share little beyond the roster lookup.

## What would close it

Split the two paths: `open_local_conversation` and `open_remote_conversation`, with the roster lookup and the override read staying here. That is ~40 raw lines out and makes the asymmetry (an override is only honourable locally) structural rather than a branch in the middle.
