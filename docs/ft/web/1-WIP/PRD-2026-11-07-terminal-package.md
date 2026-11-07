# Terminal package (`tddy-terminal-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 8/16 — branch `feature/web-split/terminal`

## Affected Features

- **Primary**: [web-terminal.md](../web-terminal.md), [terminal-replay-lazy-scroll.md](../terminal-replay-lazy-scroll.md),
  [enqueued-input-overlay.md](../enqueued-input-overlay.md), terminal zoom, OSC 52 clipboard, file upload/drop.
- **Related**: session terminal tabs (consume the terminal).

## Summary

Move the browser terminal into `packages/tddy-terminal-web`: `GhosttyTerminal`,
`GhosttyTerminalSession`, the terminal chrome in `components/connection/` (status bar, shortcut
drawer, mobile keyboard, file drop zone, upload button, enqueued-input overlay), the transport-agnostic
terminal feed and input delivery (`rpc/connections/{terminal, terminalFeed, inputDelivery}`, terminal
control and enqueued-input hooks), and the `lib/terminal*`, `osc52`, `ghosttyInstance`,
`toolShortcuts` helpers. Its ~35 Cypress component specs and drivers move with it. `ghostty-web` becomes
this package's dependency.

## Background

The terminal is the most reused surface (session tabs, dormant agent tabs, PR-stack chat terminal) and
already talks to the daemon only through the terminal RPC — a clean seam.

## Proposed Changes

### What's Changing

- New package; the oversized-file records for `GhosttyTerminal.tsx` and `GhosttyTerminalSession.tsx`
  move with the files; its PR check.

### What's Staying the Same

- Terminal behaviour (rendering, DPR, zoom, resize, history paging, input ack, OSC 52, uploads).
- The e2e terminal specs stay in `tddy-web` (they run against the Storybook build).

## Acceptance Criteria

- [ ] The terminal lives in `tddy-terminal-web`, importing nothing from `tddy-web` or LiveKit.
- [ ] Its specs pass from the package's Cypress config and PR check.
- [ ] Sessions render the terminal unchanged.

## References

- Predecessors (real): `clients`, `ui`. Successor: `session`, `storybook` (terminal stories).
