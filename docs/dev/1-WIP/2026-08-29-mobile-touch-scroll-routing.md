# Changeset: Mobile touch scroll routing — a full-screen TUI scrolls itself

**Date**: 2026-08-29
**Status**: 🚧 In Progress — code + specs written, **not yet run** (no JS toolchain in this workspace)
**Type**: Fix

## Affected Packages

- **tddy-web**: [README.md](../../packages/tddy-web/README.md)
  - `src/components/GhosttyTerminal.tsx` — the single-finger drag effect gains a route, sampled at
    `touchstart`: `tui` (alternate screen, with or without mouse tracking) hands the drag to the
    terminal as wheel notches at the touch point; `viewport` (normal screen) keeps scrolling the
    pane's own scrollback as before
  - `cypress/support/drivers/ghosttyTerminalDriver.tsx` — `expectInAlternateScreen()`,
    `expectSentToApp()`, `expectDidNotSendToApp()`
  - `cypress/component/TerminalTouchScrollRoutingAcceptance.cy.tsx` (new) — 3 specs
  - changeset index entry still to write (packages/*/docs is not edited directly)

## Related Feature Documentation

- [Web terminal § Mobile UX](../ft/web/web-terminal.md) — the touch-scrolling contract
- [Terminal replay — lazy scroll](../ft/web/terminal-replay-lazy-scroll.md) — touch joins the
  existing three-way wheel gate

## Problem

Desktop gates the wheel three ways — mouse-tracking TUI → SGR wheel report; alternate screen
without tracking → ghostty-web's native Up/Down emulation for pagers; normal screen → the
overlay double buffer's forward fill (`GhosttyTerminalGrpc`, capture phase).

Touch had no gate at all. Every one-finger drag called `scrollLines` on the pane it started in.
In a full-screen TUI — the Claude CLI runs in the alternate screen with mouse tracking on — that
scrolls the **live pane**, which is deliberately `scrollback: 0`, so the gesture does nothing
whatsoever: the TUI never learns the user tried to scroll, and the history double buffer (reachable
from the "Load earlier output" affordance) is the only way back through the output. On desktop the
same intent reaches the TUI and it scrolls its own transcript.

## Change

The drag effect samples `hasMouseTracking()` and the wasm terminal's `isAlternateScreen()` once, at
`touchstart` (so a gesture is never split between routes), and picks:

- **`tui`** — tracking **or** alternate screen: for each line of finger travel, dispatch one
  `wheel` event on the grid canvas at the touch point (`deltaY = ±cellHeight`, capped at
  `TOUCH_WHEEL_MAX_NOTCHES_PER_MOVE` per `touchmove`). Dispatching on the canvas is what a desktop
  wheel over the terminal does, so every existing handler applies unchanged and in the same order:
  ghostty-web's own wheel handling, the SGR forwarding on this container, and the capture-phase
  gate on the panes above it. No new routing logic, and no second copy of the gate to drift.
- **`viewport`** — everything else: `term.scrollLines(-lines)`, exactly as before.

## Tests

`cypress/component/TerminalTouchScrollRoutingAcceptance.cy.tsx`:

1. alternate screen + mouse tracking → drag reports SGR wheel-up (`\x1b[<64;`) and **no** Up-arrow
2. alternate screen, no tracking → drag reaches the terminal's wheel handling, which emulates
   Up-arrows (`\x1b[A`) for the pager
3. normal screen (regression) → drag scrolls the pane's scrollback and sends the application nothing

(1) and (2) fail on master: the drag produced no wheel at all, so neither byte was ever sent.

## Open

- ⚠️ **Unverified**: this workspace has no `bun`/`node_modules`/nix shell, so
  `bun run cypress:component` and `tsc --noEmit` have not been run. Both must pass before wrap,
  together with the existing touch specs (`GhosttyTerminal.cy.tsx` drag test,
  `TerminalTapMouseClickAcceptance.cy.tsx`).
- **Not addressed** (separate defect, deliberately out of this change): with mouse tracking on, the
  capture-phase tap handlers still report an SGR **press** at `touchstart` and a **release** at
  `touchend` for a gesture that turns out to be a scroll — a stray click at the point the swipe
  started. Fixing it means deferring the press until the gesture is known to be a tap.
- The forward fill still has **no touch trigger** in the normal screen (mobile reaches older
  history through the "Load earlier output" affordance). Desktop reaches it with a wheel-up at the
  tip; giving the drag the same trigger would complete the parity, and was left out of this change.
