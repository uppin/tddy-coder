# oversized-file: GhosttyTerminal.tsx

**Location:** `packages/tddy-web/src/components/GhosttyTerminal.tsx`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` on branch `claude-cli-fullscreen`
**Metrics:** **1354 lines** (whole file, TS) · budget 500 — `wc -l packages/tddy-web/src/components/GhosttyTerminal.tsx`
**Thresholds breached:** length 1354 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 1354 | first detection: 1351 at base `ba308046f`, +3 on `claude-cli-fullscreen` (the `onData` dependency of the mouse-listener effect replaced by `onDataRef` / `hasOnData`). Split deferred with the developer's consent: the growth is debug tracing for the Claude fullscreen scroll investigation, and a hand split of this component is its own behaviour-preserving PR — see [the backlog entry](../../../../docs/dev/todo/2026-10-24-ghostty-terminal-components-over-budget.md) |

## What would close it

A hand TS split under `/pr-wrap` step 3.5's discipline (the restructure engine is Rust-only): green
baseline of the terminal component specs before, mechanical moves only, same green after. The
listeners and handle methods are the natural seams (mouse/touch forwarding, wheel gate, history
double buffer), each into a hook beside the component.
