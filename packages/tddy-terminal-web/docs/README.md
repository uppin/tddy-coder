# tddy-terminal-web documentation

The browser terminal: `GhosttyTerminal`, `GhosttyTerminalSession`, the terminal chrome in
`src/connection/` (status bar, shortcut drawer, mobile keyboard, file drop and upload button,
enqueued-input overlay), the transport-agnostic terminal feed and input delivery (`src/rpc/`,
`useTerminalControl`, `useEnqueuedInput`) and the terminal helpers in `src/lib/` (zoom, OSC 52,
tool shortcuts, stream offsets). It imports nothing from `tddy-web` or `livekit-client`
(`src/packageBoundary.test.ts`); the session runtime it reports bytes to is reached through the
`onBytes(ByteDelta)` prop, and the connection status it shows is passed in as text.

- **Session terminal (feed, resync, history paging, input ack)**: [terminal-session.md](terminal-session.md)
- **Connection chrome (status bar, layouts)**: [terminal-connection-chrome.md](terminal-connection-chrome.md)
- **Presentation modes (reconnect overlay)**: [terminal-presentation.md](terminal-presentation.md)
- **Font zoom**: [terminal-zoom.md](terminal-zoom.md)
- **OSC 52 clipboard writes**: [terminal-osc52-clipboard.md](terminal-osc52-clipboard.md)
- **File upload (drop / mobile Attach)**: [terminal-file-upload.md](terminal-file-upload.md)
- **Changeset history**: [changesets/](changesets/)
- **Open code issues**: [code-issues/](code-issues/)

## Tests

`bun test src` (helpers, feed, input delivery, the boundary rule) and `bun run cypress:component` —
21 component specs from the package's own `cypress.config.ts`. Its PR check is
`.github/workflows/web-tddy-terminal-web.yml`.
