# Terminal package (`tddy-terminal-web`)

**Package:** `packages/tddy-terminal-web`
**Applies to:** every surface that shows a terminal — session terminal tabs, dormant agent tabs, the
PR-stack chat terminal, the standalone connection screen
**Added:** 2026-11-07 (#web-split 8/17, PR #845)

## Overview

The browser terminal is one package: rendering (xterm.js), the session terminal with its chrome,
the terminal feed and input delivery over the terminal RPC, and the helpers for zoom, OSC 52 and
uploads. It reaches daemons only through `tddy-client-web` and renders with `tddy-ui-web`; it does not
depend on the web app or on LiveKit.

## Behaviour

- Every terminal behaviour is as before ([web-terminal.md](web-terminal.md),
  [terminal-replay-lazy-scroll.md](terminal-replay-lazy-scroll.md),
  [enqueued-input-overlay.md](enqueued-input-overlay.md),
  [session-terminal-tabs.md](session-terminal-tabs.md)).

## Acceptance criteria

- [x] The terminal lives in `tddy-terminal-web` and imports nothing from `tddy-web` or LiveKit (rule test).
- [x] Its specs pass from the package's own Cypress config and PR check.
- [x] Sessions render the terminal unchanged (`tddy-web`'s terminal-rendering specs pass).

Technical reference: [packages/tddy-terminal-web/docs/README.md](../../../packages/tddy-terminal-web/docs/README.md).
