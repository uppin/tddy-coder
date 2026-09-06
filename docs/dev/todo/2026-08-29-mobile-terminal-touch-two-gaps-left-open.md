# 2026-08-29 — Mobile terminal touch — two gaps left open

**Category:** Future enhancement
**Source:** mobile-touch-scroll-routing changeset, 2026-08-29

- **A scroll gesture still reports a stray click to a mouse-tracking TUI.** The capture-phase tap
  handlers send an SGR **press** at `touchstart` and a **release** at `touchend` for every
  single-finger gesture, including one that turns out to be a drag — so a swipe that starts on a
  clickable affordance in the Claude CLI's TUI presses it. The desktop wheel sends no press at all.
  Fixing it means deferring the press until the gesture is known to be a tap (the threshold the
  tap-synthesis effect already measures), which changes when a genuine tap is reported and needs the
  existing `expectSgrPressAndReleaseReportedOnce` contract re-pinned rather than assumed.
- **The lazy-history forward fill has no touch trigger in the normal screen.** Desktop starts it
  with a wheel-up at the tip; mobile reaches older output only through the "Load earlier output"
  affordance. Giving the drag the same trigger would complete the parity — deliberately left out of
  the routing fix, which was about a full-screen TUI owning its own scrolling.
