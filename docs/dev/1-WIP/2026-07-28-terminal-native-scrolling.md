# Changeset: Terminal Replay — Native Scrollbar & Scroll-up Gating (scrollback 0 retained)

**Date**: 2026-07-28
**Status**: ✅ Implemented — green phase complete (acceptance + unit tests green)
**Type**: Feature (amends `2026-07-28-terminal-replay-viewport`)

## Planning artifacts

- [x] PRD: [docs/ft/web/terminal-replay-lazy-scroll.md](../../docs/ft/web/terminal-replay-lazy-scroll.md)
  (amended — "Updated: 2026-07-28 (scrollback-0 retained)" block).
- [x] Changeset: this document.
- [x] Acceptance tests (Cypress component): extend
  `cypress/component/GhosttyTerminalGrpcLazyHistory.cy.tsx` + driver
  `cypress/support/drivers/ghosttyTerminalGrpcLazyHistoryDriver.tsx` (18 passing).
- [x] Unit tests (bun): `src/lib/terminalScrollbar.test.ts` for the pure
  `computeScrollbar(scrollbackLength, viewportY, rows)` helper (6 passing).

## Affected Packages

- `tddy-web` only — no proto/RPC/daemon changes. The lazy forward-fill wire contract
  (`StreamTerminalOutput` last-frame-first + `GetTerminalHistory` forward chunks) is unchanged.

## Related Feature Documentation

- [terminal-replay-lazy-scroll.md](../../docs/ft/web/terminal-replay-lazy-scroll.md) — the WHAT,
  amended by this changeset (native viewport model, scrollback 0 retained).
- [terminal-sessions.md (daemon) § Lazy replay & scroll-up history](../../docs/daemon/terminal-sessions.md#lazy-replay--scroll-up-history-added-2026-07-28)
  — the wire contract this consumes (unchanged).

## Summary

Brings the **native ghostty `Scrollbar`** to the page terminal and locks the live terminal to
**`scrollback: 0`** (the original no-duplicate-pane mitigation is retained). The earlier
"terminal-native-scrolling" amendment proposed raising the live terminal to `scrollback > 0` to
match native ghostty's primary screen; that change was **reverted** because it reintroduced
duplicate-pane accumulation for TUIs that full-screen repaint on the primary screen and surfaced a
blank-page-after-reconnect bug in the forward-fill. This changeset captures the parts that stayed
plus the regression fix.

- **Live terminal = the active screen, pinned to the tip.** It stays at `scrollback: 0` (no native
  scrollback). A wheel-up therefore fires the page forward-fill **immediately** — the user does not
  first scroll through post-connect history in the live terminal (there is none). The
  duplicate-pane mitigation is the global `scrollback: 0`, which is stricter than native (native
  primary screen has scrollback) but is the only way to stop a primary-screen-repainting TUI from
  accumulating duplicates in the live scrollback.
- **Page terminal = the older PageList history view.** It keeps `scrollback > 0` and is
  forward-filled with pre-connect history older than what the live terminal holds. It exposes a
  native `Scrollbar { total, offset, len }` — the single source of truth for viewport position, in
  the same coordinate space as `scrollToLine` — matching ghostty's `PageList.Scrollbar`
  (`tmp/ghostty/src/terminal/PageList.zig:3339`): `total = getScrollbackLength() + rows`,
  `offset = max(0, getScrollbackLength() - getViewportY())`, `len = rows`.
- **Forward-fill bounded by the current live tip, not the stale anchor.** The anchor captured at
  first connect becomes stale after a reconnect; if the capture ring evicts past it,
  `replay_from(0, anchor)` returns an empty range. The fill is now bounded by the **current
  cumulative output offset** at fill time (`currentOffsetRef`), so a reconnect that advanced the tip
  still fetches the retained history. A fill that resolves with **no bytes written** (empty/evicted
  range) or that **errors** (e.g. sandbox `getTerminalHistory not_found`) **stays on the live pane**
  — no blank page swap. The loading indicator clears and the affordance remains available for
  retry.
- **Mouse-tracking gating (native).** When the TUI has enabled mouse tracking (DEC 1006), the
  wheel is reported to the TUI (SGR button 64/65) and does **not** scroll the viewport or trigger
  the forward-fill — matching ghostty's `isMouseReporting` gate (`Surface.zig:3605`). Already
  implemented in `GhosttyTerminal.tsx` `onWheel`; this changeset adds test coverage (with
  `scrollback: 0` the viewport stays pinned at the tip regardless).

No RPC change. No proto change. No daemon change.

## Scope

- [x] `GhosttyTerminal`: add `getScrollbar()` to the imperative handle
  (`{ total, offset, len }` with `total = getScrollbackLength() + rows`,
  `offset = max(0, getScrollbackLength() - getViewportY())`, `len = rows`).
- [x] `GhosttyTerminalGrpc`: live terminal stays at `scrollback: 0` (no `LIVE_SCROLLBACK` constant);
  the scroll-up-on-live gesture uses `isPinnedToBottom()` (always true at `scrollback: 0`); page
  terminal `getScrollbar()` mirrored to a hidden `terminal-page-scrollbar` element for tests;
  `startForwardFill` bounded by `currentOffsetRef.current` with `wroteAny`/`failed` gating the
  page swap.
- [x] New pure helper `src/lib/terminalScrollbar.ts`:
  `computeScrollbar(scrollbackLength, viewportY, rows) → { total, offset, len }`.
- [x] Acceptance tests (Cypress component) for the scrollback-0 + native Scrollbar + mouse-tracking
  + blank-reconnect regression behaviors.
- [x] Bun unit tests for `computeScrollbar`.

## Technical Changes

### State A (before — `terminal-replay-viewport`)

- Live terminal constructed with `scrollback: 0` (default); viewport APIs are no-ops on it.
- Scroll-up-on-live gesture fires on any wheel-up while pinned to bottom.
- Page terminal exposes only `viewportY` (lines scrolled up from bottom) via
  `terminal-page-viewport-y`; no native `Scrollbar { total, offset, len }`.
- `startForwardFill` bounded by the anchor's `endOffset` and swaps to the page terminal
  unconditionally in `finally` — a stale/evicted anchor yields a blank page.

### State B (after)

- Live terminal stays at `scrollback: 0`; the scroll-up-on-live gesture fires on any wheel-up
  (`isPinnedToBottom()` is always true).
- Page terminal exposes native `Scrollbar { total, offset, len }` via `getScrollbar()` and a
  hidden `terminal-page-scrollbar` mirror.
- `startForwardFill` is bounded by `currentOffsetRef.current` (the current live tip), tracks
  `wroteAny`/`failed`, and swaps to the page terminal only when `!failed && wroteAny`. A failed or
  empty fill clears the loading indicator and keeps the live pane foreground (no blank page).
- Mouse tracking on → wheel to TUI (no viewport scroll, no forward-fill); viewport stays pinned at
  the tip.

### Delta

- `packages/tddy-web/src/lib/terminalScrollbar.ts`: new pure `computeScrollbar`.
- `packages/tddy-web/src/components/GhosttyTerminal.tsx`: `getScrollbar()` handle method.
- `packages/tddy-web/src/components/GhosttyTerminalGrpc.tsx`: `startForwardFill` rewrite
  (current-tip bounding + `wroteAny`/`failed` no-blank-swap); scroll-up-on-live gesture via
  `isPinnedToBottom()`; page terminal Scrollbar mirror; `scrollback: 0` retained on live.
- `packages/tddy-web/cypress/support/testIds.ts`: new id `terminalPageScrollbar`.
- Cypress: extend `GhosttyTerminalGrpcLazyHistory.cy.tsx` + driver with scrollback-0,
  native-Scrollbar, mouse-tracking, and blank-reconnect regression tests.
- Bun: `src/lib/terminalScrollbar.test.ts`.

## Acceptance Tests

All Cypress component tests use a fake `GrpcStream` + `historyFetcher` double (never
`cy.intercept`). Fluent-tests style (Given/When/Then, named driver helpers, one behavior per test,
no raw selectors in bodies).

1. "Live terminal has no native scrollback — overflow output does not accumulate, and a scroll-up
   gesture triggers the page forward-fill immediately" (fetch called with `(0, currentTip)`, not
   `(0, anchor)`).
2. "Live terminal stays at scrollback 0 across primary and alternate screen (DEC 1049) — alt-screen
   repaints do not accumulate scrollback".
3. "Page terminal exposes the native Scrollbar { total, offset, len } as the single source of truth
   and scrollToLine sets the absolute offset".
4. "Mouse tracking on (DEC 1006) routes the wheel to the TUI, not the viewport — viewport stays at
   scrollback 0, no forward-fill".
5. **Regression**: "After a reconnect that advances the tip past the original anchor, a scroll-up
   forward-fills bounded by the current tip (not the stale anchor) and the page shows retained
   history".
6. **Regression**: "A forward fill that resolves with no retained bytes (empty/evicted range) stays
   on the live pane — no blank page; the affordance remains visible".
7. **Regression**: "A forward fill whose fetch errors stays on the live pane — no blank page; the
   affordance remains visible".

Bun unit tests (`terminalScrollbar.test.ts`) for `computeScrollbar`:
- bottom (viewportY=0 → offset = scrollbackLength, total = scrollbackLength + rows, len = rows);
- top (viewportY = scrollbackLength → offset = 0);
- mid (viewportY = K → offset = scrollbackLength - K);
- clamping (viewportY > scrollbackLength → offset = 0; viewportY < 0 → offset = scrollbackLength);
- zero-scrollback (scrollbackLength=0 → total = rows, offset = 0, len = rows).

## Technical Debt & Production Readiness

- The live terminal's `scrollback: 0` is stricter than native ghostty (native primary screen has
  scrollback). A TUI that repaints on the primary screen would duplicate in native too; the global
  `scrollback: 0` is the only way to keep the live pane duplicate-free for primary-screen repaints.
  Accepted per user decision (duplicate-free live pane wins over native parity).
- The page terminal is still filled with the entire retained capture (`0 → currentTip`); paged
  forward-fill remains a future optimization.

## Decisions & Trade-offs

See PRD § Decisions & trade-offs (amended — scrollback-0 retained).

## References

- [PRD](../../docs/ft/web/terminal-replay-lazy-scroll.md)
- [Native ghostty analysis](../../tmp/ghostty/src/terminal/PageList.zig) (cloned third-party repo)
- `packages/tddy-web/src/components/GhosttyTerminal.tsx`
- `packages/tddy-web/src/components/GhosttyTerminalGrpc.tsx`
- `packages/tddy-web/node_modules/ghostty-web/dist/index.d.ts` (`IBuffer.length`,
  `getScrollbackLength`, `scrollToLine`, `getViewportY`)
