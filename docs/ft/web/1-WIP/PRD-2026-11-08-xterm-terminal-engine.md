# xterm.js terminal engine - PRD

**Date**: 2026-11-08
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#xterm` 3/4 — branch `feature/xterm/xterm-engine`

## Affected Features

- Every web terminal (session terminals, dormant agent tab, standalone connection page, storybook).

## Summary

Replace ghostty-web with xterm.js as the web terminal's engine: `xtermEngine` implements `TerminalEngine` with `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-unicode11` and `@xterm/addon-webgl` (WebGL renderer, the DOM renderer when WebGL is unavailable). The ghostty-only workarounds are deleted, and the canvas-geometry specs are rewritten.

## Background

ghostty-web needs a per-terminal WASM instance, a DPR watcher on a private renderer field, hand-written SGR mouse reporting, a capture-phase wheel gate and an IME Escape workaround. xterm.js handles DPR, mouse reporting and alternate-screen wheel itself, has a public API for buffers and modes, and is the widely used browser terminal. Developer decisions (2026-11-08): WebGL with DOM fallback; the five dependencies above.

## Proposed Changes

### What's Changing

- `xtermEngine` (`src/engine/xtermEngine.ts`) becomes the only engine the components construct; `xterm.css` is loaded by the package.
- Renderer: `@xterm/addon-webgl` when a WebGL context is available; otherwise xterm's DOM renderer (consented fallback). The active renderer is exposed as `data-terminal-renderer`.
- Deleted: `lib/ghosttyInstance.ts` use, `lib/terminalDprWatch.ts`, hand SGR mouse reporting (`lib/terminalMouseCellCoords.ts` except the touch→wheel route), the capture-phase wheel gate, the IME Escape forward (if xterm's composition handling covers it), write buffering before ready.
- OSC 52 through the existing `lib/osc52.ts` write tap (no `@xterm/addon-clipboard`: the tap answers no clipboard reads).
- Unicode 11 widths active.
- Touch routes and the fixed-grid font fit measure `.xterm-screen` instead of a canvas.
- Canvas-based specs rewritten: DPR spec deleted, isolation spec repurposed, shrink spec measures `.xterm-screen`, selection screenshot replaced by a buffer/selection assertion.

### What's Staying the Same

- The terminal's visible behaviour: input, paste, file drop, OSC 52, zoom, mobile keyboard and shortcut drawer, scrollback, the lazy history double buffer, snapshot on reconnect.
- `ghosttyEngine` still exists (deleted in n4).

## Acceptance Criteria

1. Every terminal constructs `xtermEngine`; no component constructs `ghosttyEngine`.
2. The engine contract suite passes against `xtermEngine`.
3. A mouse-tracking TUI receives exactly one report per click and per wheel step (no double reporting).
4. Wide characters (emoji, CJK) occupy two cells, matching the TUI's layout.
5. The renderer is WebGL where available and DOM otherwise, and `data-terminal-renderer` says which.
6. All terminal specs in `tddy-terminal-web`, `tddy-session-web` and `tddy-web` pass, and the storybook terminal e2e specs pass.

## References

- Stack: `#xterm` (ghostty-web → xterm.js), stacked on `feature/web-split/wiring` (#853).
- Predecessor: `feature/xterm/terminal-engine`.
