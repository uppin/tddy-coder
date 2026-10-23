# Changeset: terminal-manual-history-toggle

PRD (target): [`docs/ft/web/terminal-replay-lazy-scroll.md`](../../ft/web/terminal-replay-lazy-scroll.md)
(amended 2026-10-09 — *Manual history toggle*)

## Problem

The web terminal switched to the history (page) pane on its own: any wheel-up on the live pane
started the forward fill — or, once filled, swapped to history instantly — and a wheel-down at the
bottom of the page pane swapped back. Scrolling inside a full-screen TUI that does not mouse-track
kept dropping the operator out of the live view, and nothing about the history pane said it was
not live.

## State B

- The wheel never switches panes. The live pane's capture-phase wheel listener only routes the
  wheel to a mouse-tracking TUI (SGR 64/65); the page pane has no wheel listener at all.
- The existing affordances are the toggle: **Load earlier output** / **View history** on the live
  pane, **Back to live** on the page pane.
- The page pane renders in a **greyish color scheme**: the page terminal is constructed with a grey
  theme (`HISTORY_THEME`) and the pane carries `filter: grayscale(1) brightness(0.9)`
  (`HISTORY_FILTER`) plus `data-color-scheme="history"`. The filter is what desaturates the replayed
  output's own SGR and truecolor colors, which a theme cannot reach.

## Package docs to update at wrap

- `packages/tddy-web/docs/terminal-session.md` § *What the component still owns* — "swaps the two
  panes on a scroll-up-at-top gesture or the explicit affordance" becomes "swaps the two panes on
  the explicit toggle only, rendering history in a greyish scheme"; "routes the wheel three ways"
  becomes "routes the wheel to a mouse-tracking TUI".

## Files

- `packages/tddy-web/src/components/GhosttyTerminalSession.tsx`
- `packages/tddy-web/cypress/component/TerminalHistoryPaging.cy.tsx`
- `packages/tddy-web/cypress/support/drivers/terminalHistoryPagingDriver.tsx`
