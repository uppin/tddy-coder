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
`touchstart` (so a gesture is never split between routes), and picks, per line of finger travel
(capped at `TOUCH_SCROLL_MAX_NOTCHES_PER_MOVE` per `touchmove` so a fling cannot flood the TUI):

- **`tui-mouse`** — the TUI tracks the mouse: one SGR wheel report (button 64 up / 65 down) at the
  touch point, through the new component-scope `sendWheelSgrAt` that the `sendWheelSgr` handle
  method (the desktop wheel path) now also calls — one implementation, two callers.
- **`tui-keys`** — alternate screen without tracking: the arrow key ghostty-web emulates the wheel
  with there (`\x1b[A` / `\x1b[B`).
- **`viewport`** — everything else: `term.scrollLines(-lines)`, exactly as before.

**First attempt, and why it is not this one.** The drag originally re-dispatched a synthetic `wheel`
on the grid canvas and left the routing to the handlers that already gate the desktop wheel
(ghostty-web's own, the SGR forwarding on the container, `GhosttyTerminalGrpc`'s capture-phase
gate) — no second copy of the gate to drift. CI showed the event reached none of them: the
mouse-tracking spec saw only the tap's press/release (`\x1b[<0;28;5M` … `13m`) and the pager spec
saw nothing at all, while the normal-screen spec passed — so the drag handler was running and the
canvas rect was measurable (the tap coordinates come from the same rect), and it was the dispatch
that landed nowhere. Sending to the application directly is what the two specs can actually pin.

## Tests

`cypress/component/TerminalTouchScrollRoutingAcceptance.cy.tsx`:

1. alternate screen + mouse tracking → drag reports SGR wheel-up (`\x1b[<64;`) and **no** Up-arrow
2. alternate screen, no tracking → drag reaches the terminal's wheel handling, which emulates
   Up-arrows (`\x1b[A`) for the pager
3. normal screen (regression) → drag scrolls the pane's scrollback and sends the application nothing

(1) and (2) fail on master: the drag produced no wheel at all, so neither byte was ever sent.

## Open

- ⚠️ **Verified only in CI**: this workspace has no `bun`/`node_modules`/nix shell, so
  `bun run cypress:component` and `tsc --noEmit` cannot be run here. The first CI run failed both
  alt-screen specs (see "First attempt" above) and passed the normal-screen one; the second run is
  the check on the rewrite, together with the existing touch specs (`GhosttyTerminal.cy.tsx` drag
  test, `TerminalTapMouseClickAcceptance.cy.tsx`).
- **Not addressed** (separate defect, deliberately out of this change): with mouse tracking on, the
  capture-phase tap handlers still report an SGR **press** at `touchstart` and a **release** at
  `touchend` for a gesture that turns out to be a scroll — a stray click at the point the swipe
  started. Fixing it means deferring the press until the gesture is known to be a tap.
- The forward fill still has **no touch trigger** in the normal screen (mobile reaches older
  history through the "Load earlier output" affordance). Desktop reaches it with a wheel-up at the
  tip; giving the drag the same trigger would complete the parity, and was left out of this change.
