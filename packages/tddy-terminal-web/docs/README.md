# tddy-terminal-web documentation

The browser terminal: `TerminalView`, `TerminalSession`, the terminal chrome in
`src/connection/` (status bar, shortcut drawer, mobile keyboard, file drop and upload button,
enqueued-input overlay), the transport-agnostic terminal feed and input delivery (`src/rpc/`,
`useTerminalControl`, `useEnqueuedInput`) and the terminal helpers in `src/lib/` (zoom, OSC 52,
tool shortcuts, stream offsets). It imports nothing from `tddy-web` or `livekit-client`
(`src/packageBoundary.test.ts`); the session runtime it reports bytes to is reached through the
`onBytes(ByteDelta)` prop, and the connection status it shows is passed in as text.

- **Terminal engine (interface, top-based scrolling, fitting, buffer text)**: [terminal-engine.md](terminal-engine.md)
- **xterm.js engine (renderer, what xterm.js owns, what stays around it)**: [xterm-engine.md](xterm-engine.md)
- **Session terminal (feed, resync, history paging, input ack)**: [terminal-session.md](terminal-session.md)
- **Connection chrome (status bar, layouts)**: [terminal-connection-chrome.md](terminal-connection-chrome.md)
- **Presentation modes (reconnect overlay)**: [terminal-presentation.md](terminal-presentation.md)
- **Font zoom**: [terminal-zoom.md](terminal-zoom.md)
- **OSC 52 clipboard writes**: [terminal-osc52-clipboard.md](terminal-osc52-clipboard.md)
- **File upload (drop / mobile Attach)**: [terminal-file-upload.md](terminal-file-upload.md)
- **Changeset history**: [changesets/](changesets/)

## Source layout

`TerminalView.tsx` and `TerminalSession.tsx` are compositions of hooks; every production
file stays within 500 lines (`src/componentSize.test.ts`).

- `src/engine/` — the `TerminalEngine` interface, `xtermEngine` (the only xterm.js importer)
  and the reusable engine contract suite.
- `src/terminal/` — what the emulator component is made of: `types`, `terminalLoggers`,
  `terminalGeometry`, `terminalRefs`, `useTerminalLifecycle` (open, fit, write, dispose),
  `useTerminalFontSize` (font size, fit and the zoom bridge), `useTerminalPinchZoom`,
  `useTerminalTouchScroll` (touch drag scroll routes and SGR wheel),
  `useTerminalTouchFocus` (focus on tap, tap as click, textarea attributes),
  `useTerminalKeyForwarding` (desktop keys and IME escape) and `useTerminalHandle` (the imperative
  handle).
- `src/session/` — what the session component is made of: `types`, `sessionDiagnostics`,
  `useFeedStream` (feed and session end), `useHistoryDoubleBuffer`, `useTerminalMirrors`
  (buffer text and viewport), `useLivePaneGates` (test window hooks),
  `TerminalProbes`, `PaneOverlays` and `paneStyles`.

## Tests

`bun test src` (helpers, feed, input delivery, the boundary rule) and `bun run cypress:component` —
22 component specs from the package's own `cypress.config.ts`. CI: `.github/workflows/web.yml`
(`Web lint` typechecks it; the `Web tests` shards run its suites).
