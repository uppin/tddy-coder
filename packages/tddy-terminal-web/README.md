# tddy-terminal-web

The browser terminal: `TerminalView`, `TerminalSession`, the terminal chrome (status bar,
shortcut drawer, mobile keyboard, file drop and upload, enqueued-input overlay), the transport-agnostic
terminal feed and input delivery, and the terminal helpers (`lib/terminal*`, OSC 52, zoom).

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

Tests: `bun test src` and `bun run cypress:component` (specs mount with `cy.mountWithClients`).
