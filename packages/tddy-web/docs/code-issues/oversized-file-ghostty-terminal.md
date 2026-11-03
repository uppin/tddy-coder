# oversized-file: GhosttyTerminal.tsx

**Location:** `packages/tddy-web/src/components/GhosttyTerminal.tsx`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` on branch `claude-cli-fullscreen`
**Metrics:** **1479 lines** (whole file, TS) · budget 500 — `wc -l packages/tddy-web/src/components/GhosttyTerminal.tsx`
**Thresholds breached:** length 1479 > 500
**Status:** Open — unclaimed
**Deferred:** by the developer 2026-11-03 (#session-tabs sweep: code issues deferred, logic issues solved)

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 1354 | first detection: 1351 at base `ba308046f`, +3 on `claude-cli-fullscreen` (the `onData` dependency of the mouse-listener effect replaced by `onDataRef` / `hasOnData`). Split deferred with the developer's consent: the growth is debug tracing for the Claude fullscreen scroll investigation, and a hand split of this component is its own behaviour-preserving PR — see [the backlog entry](../../../../docs/dev/todo/2026-10-24-ghostty-terminal-components-over-budget.md) |
| 2026-10-25 | 1362 | +8 on #765 (`fix-desktop-terminal-file-drop`), measured at its wrap against `origin/master` `a618ef29d`: the handle's `paste` (ghostty-web's bracketed-aware paste) and its doc. Not split there |
| 2026-11-02 | 1479 | +12 on #807 (`osc52-clipboard`), measured at its wrap against its base (1467): the OSC 52 recognizer import, one `useMemo`, and a tap beside each of the three writes. The recognizer itself is `src/lib/osc52.ts`. Split not done in this node (a stack node among several touching terminal code); the `initialContent` double write was recorded in the backlog (closed by #813) |
| 2026-11-03 | 1475 | −4 on #813 (`#session-tabs` sweep), measured against `origin/master` `162a66978` (1479): the setup effect's duplicate `initialContent` write + tap removed (that backlog entry is closed). Still over budget; split deferred by the developer |

## What would close it

A hand TS split under `/pr-wrap` step 3.5's discipline (the restructure engine is Rust-only): green
baseline of the terminal component specs before, mechanical moves only, same green after. The
listeners and handle methods are the natural seams (mouse/touch forwarding, wheel gate, history
double buffer), each into a hook beside the component.
