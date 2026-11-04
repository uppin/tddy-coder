# oversized-file: svc_start_session_core.rs

**Location:** `packages/tddy-agent-launch/src/svc_start_session_core.rs`
**Category:** oversized-file
**Detected:** 2026-11-04 by `/pr-wrap` step 3.5 file-length gate on PR #801 (`#agent-tools` 12/19)
**Metrics:** **502 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.00× over**
**Thresholds breached:** length 502 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-04 | 497 → 502 | first detection — PR #801 crossed it: `start_here` reads the launch path from the host's registry (`cli_launcher`) and the generic branch refuses a host without coding agents; `classify_start_placement` and `refuse_a_cli_start_without_a_model` take the host's agents. Not split: the developer deferred all code restructuring of the `#agent-tools` stack to its sweep node (2026-11-04) |

## What would close it

The per-session-type branches of `start_here` (claude, cursor, generic) are what n17 `cli-dispatch-collapse` ([#823](https://github.com/uppin/tddy-coder/pull/823)) collapses into one spec-driven start; see also `complexity-svc-start-session-core-start-session-core.md`. Re-measure when n17 lands.
