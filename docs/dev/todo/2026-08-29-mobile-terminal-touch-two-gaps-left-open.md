# 2026-08-29 — Mobile terminal touch — one gap left open

**Category:** Future enhancement
**Source:** mobile-touch-scroll-routing changeset, 2026-08-29

- **The lazy-history forward fill has no touch trigger in the normal screen.** Desktop starts it
  with a wheel-up at the tip; mobile reaches older output only through the "Load earlier output"
  affordance. Giving the drag the same trigger would complete the parity — deliberately left out of
  the routing fix, which was about a full-screen TUI owning its own scrolling.

## Closed: the stray click on a scroll gesture (2026-11-08, `#xterm` 3/4, PR #868)

The capture-phase SGR touch handlers are gone with the move to xterm.js: a tap reaches a
mouse-tracking TUI only through the tap-as-click re-synthesis, which a drag or a second finger
cancels, and xterm.js reports the synthesised click. A one-finger drag sends wheel reports and no
press (`TerminalTouchScrollRoutingAcceptance.cy.tsx`, "reports a one-finger drag to a full-screen
TUI…"). Only the gap above remains.
