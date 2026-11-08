# Terminal component split along its input, scroll and history seams - PRD

**Date**: 2026-11-08
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#xterm` 1/4 — branch `feature/xterm/terminal-seams`

## Affected Features

- `docs/ft/web/` terminal features (session terminal, scrolling, mobile touch, zoom) — behaviour unchanged.

## Summary

Split `GhosttyTerminal.tsx` (1491 lines) and `GhosttyTerminalSession.tsx` (1004 lines) in `tddy-terminal-web` into hooks and modules along their natural seams, with no behaviour change. This closes the two oversized-file code issues and gives the engine swap in the next nodes small, separately testable units.

## Background

Both components have been over the 500-line budget since 2026-10-24, and the split was deferred to "its own behaviour-preserving PR". The `#xterm` stack replaces ghostty-web with xterm.js; the code it must rewrite (mouse/touch forwarding, wheel gate, scroll mapping, history double buffer, fit/zoom) is interleaved through these two files. Splitting first keeps the engine swap reviewable.

## Proposed Changes

### What's Changing

- `GhosttyTerminal.tsx` → the component plus hooks beside it: mouse/touch forwarding (`useTerminalPointerInput`), touch drag scroll routes (`useTerminalTouchScroll`), fit and zoom (`useTerminalFit`), OSC 52 tap and IME handling (`useTerminalClipboardAndIme`), the imperative handle (`useTerminalHandle`).
- `GhosttyTerminalSession.tsx` → the component plus the wheel gate (`useWheelOwnership`), the history double buffer (`useHistoryDoubleBuffer`) and buffer-text mirroring (`useBufferTextMirror`).
- Each resulting file ≤ 500 lines.

### What's Staying the Same

- Every behaviour, prop, export, testId and spec assertion. Public names (`GhosttyTerminal*`) are unchanged until the last node.

## Acceptance Criteria

1. `GhosttyTerminal.tsx` and `GhosttyTerminalSession.tsx` are each ≤ 500 lines, and every new module is ≤ 500 lines.
2. The public exports of `tddy-terminal-web` (`src/index.ts`) are unchanged.
3. Every `tddy-terminal-web` component spec and unit test passes unchanged, and the terminal specs in `tddy-session-web` and `tddy-web` pass unchanged.
4. The two oversized-file code issues are closed.

## References

- Stack: `#xterm` (ghostty-web → xterm.js), stacked on `feature/web-split/wiring` (#853).
- Predecessor: `feature/web-split/wiring`.
