# oversized-file: GhosttyTerminalSession.tsx

**Location:** `packages/tddy-terminal-web/src/GhosttyTerminalSession.tsx`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` on branch `claude-cli-fullscreen`
**Metrics:** **1005 lines** (whole file, TS) · budget 500 — `wc -l packages/tddy-terminal-web/src/GhosttyTerminalSession.tsx`
**Thresholds breached:** length 1005 > 500
**Status:** Open — claimed
**Claimed by:** #866 — `#xterm 1/4`

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 971 | first detection: 947 at base `ba308046f`, +24 on `claude-cli-fullscreen` (`tddy:term:modes` tracing (`decModeChanges`, `traceModes`) and `tddy:term:chrome`). Split deferred with the developer's consent: the growth is debug tracing for the Claude fullscreen scroll investigation, and a hand split of this component is its own behaviour-preserving PR — see [the backlog entry](../../../../docs/dev/todo/2026-10-24-ghostty-terminal-components-over-budget.md) |
| 2026-10-24 | 992 | +21 on `feature/claude-fullscreen/hide-history-for-tuis` (#714): `tuiOwnsNavigation`, `liveTuiOwnsNavigation` and the history-offer gate. Same deferral, same stack |
| 2026-10-25 | 1006 | +14 on #765 (`fix-desktop-terminal-file-drop`), measured at its wrap against `origin/master` `a618ef29d`: the `readsLocalPaths` prop and its doc, and `pasteInput` (dropped paths delivered as a paste). Not split there — a hand split of this component remains its own behaviour-preserving PR |
| 2026-11-02 | 1005 → 1005 | Unchanged by #809 (`#session-tabs` 6/7), measured against its base `feature/session-tabs/tab-plus` `0426e14e2` (1005): `HISTORY_THEME` and `HISTORY_FILTER` are exported for the stopped Agent tab, which lives in its own module (`DormantAgentTerminal.tsx`). No logic added here |
| 2026-11-07 | 1004 → 1004 | Unchanged by #843 (`#web-split` 6/17), measured against its base `feature/web-split/clients`: its debug-mask and terminal-helper imports re-point to `tddy-client-web`. Not grown, alert-only |

## What would close it

A hand TS split under `/pr-wrap` step 3.5's discipline (the restructure engine is Rust-only): green
baseline of the terminal component specs before, mechanical moves only, same green after. The
listeners and handle methods are the natural seams (mouse/touch forwarding, wheel gate, history
double buffer), each into a hook beside the component.

## Verified by hand
2026-11-07 — `wc -l` at the wrap of PR #842 (`#web-split` 5/16): 1005 → 1005. The PR only re-points hook imports to `tddy-client-web`; still open, not split (later `#web-split` nodes move this file).
