# Terminal engine interface with top-based scrolling - PRD

**Date**: 2026-11-08
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#xterm` 2/4 — branch `feature/xterm/terminal-engine`

## Affected Features

- Terminal scrolling, scrollbar, lazy history paging, TUI mode detection — behaviour unchanged.

## Summary

Introduce a `TerminalEngine` interface in `tddy-terminal-web` that the terminal components use instead of ghostty-web's `Terminal` directly, implemented by `ghosttyEngine`. Scroll positions in the interface and everywhere above it count from the top, so the xterm.js engine (n3) drops in without touching the components.

## Background

ghostty-web counts its viewport from the bottom (0 = live tip) and is reached through `as unknown` casts for scrolling, alternate-screen and mouse-tracking queries. xterm.js counts from the top and exposes those as public API. Flipping the coordinates while ghostty is still the engine isolates that risk from the engine swap.

## Proposed Changes

### What's Changing

- `TerminalEngine` (`src/engine/TerminalEngine.ts`): open/dispose, write/paste/clear, resize/fit proposal, font size, focus, top-based scroll (`viewportTop`, `scrollbackLength`, `scrollToLine`, `scrollLines`, `scrollToTop/Bottom`), modes (`mouseTracking`, `alternateScreen`), buffer read (`readLines`, `cellInverse`), events (`onData`, `onResize`, `onBell`, `onTitleChange`, `onScroll`, `onBufferChange`), and the rendering element used for geometry.
- `ghosttyEngine` (`src/engine/ghosttyEngine.ts`) implements it, holding every ghostty cast and the bottom→top translation.
- The components, `lib/terminalScrollbar.ts`, the history mirrors and the paging driver use top-based positions.
- Specs and drivers that read canvas geometry or ghostty internals read through the engine or the `terminal-buffer-text` elements instead.

### What's Staying the Same

- Rendering, input and every user-visible behaviour; ghostty-web remains the only engine.

## Acceptance Criteria

1. No file outside `src/engine/ghosttyEngine.ts` (and `lib/ghosttyInstance.ts`) imports `ghostty-web` or casts to reach a ghostty member.
2. Scroll positions in the handle API, `terminalScrollbar` and the history paging count from the top.
3. A contract test suite for `TerminalEngine` passes against `ghosttyEngine`.
4. All terminal specs in `tddy-terminal-web`, `tddy-session-web` and `tddy-web` pass.

## References

- Stack: `#xterm` (ghostty-web → xterm.js), stacked on `feature/web-split/wiring` (#853).
- Predecessor: `feature/xterm/terminal-seams`.
