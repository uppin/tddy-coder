# oversized-file: GhosttyTerminalSession.tsx

**Location:** `packages/tddy-web/src/components/GhosttyTerminalSession.tsx`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` on branch `claude-cli-fullscreen`
**Metrics:** **1006 lines** (whole file, TS) · budget 500 — `wc -l packages/tddy-web/src/components/GhosttyTerminalSession.tsx`
**Thresholds breached:** length 1006 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 971 | first detection: 947 at base `ba308046f`, +24 on `claude-cli-fullscreen` (`tddy:term:modes` tracing (`decModeChanges`, `traceModes`) and `tddy:term:chrome`). Split deferred with the developer's consent: the growth is debug tracing for the Claude fullscreen scroll investigation, and a hand split of this component is its own behaviour-preserving PR — see [the backlog entry](../../../../docs/dev/todo/2026-10-24-ghostty-terminal-components-over-budget.md) |
| 2026-10-24 | 992 | +21 on `feature/claude-fullscreen/hide-history-for-tuis` (#714): `tuiOwnsNavigation`, `liveTuiOwnsNavigation` and the history-offer gate. Same deferral, same stack |
| 2026-10-25 | 1006 | +14 on #765 (`fix-desktop-terminal-file-drop`), measured at its wrap against `origin/master` `a618ef29d`: the `readsLocalPaths` prop and its doc, and `pasteInput` (dropped paths delivered as a paste). Not split there — a hand split of this component remains its own behaviour-preserving PR |

## What would close it

A hand TS split under `/pr-wrap` step 3.5's discipline (the restructure engine is Rust-only): green
baseline of the terminal component specs before, mechanical moves only, same green after. The
listeners and handle methods are the natural seams (mouse/touch forwarding, wheel gate, history
double buffer), each into a hook beside the component.
