# tddy-session-web

Sessions: the seams session screens reach the app through (`SessionShellProvider` /
`SessionShell`), the session helpers (indicator and notification registry, participant metadata,
labels, grouping, sorting, selection, connection status, traffic meter, transport flavour, LiveKit
ping, the notification feed), and the session runtime with its terminals — `SessionRuntime`, the
runtime registry, `GrpcSessionTerminal`, the terminal tab strip, child and dormant runtimes, tmux
sessions and attach claims. The inspector and drawer join it in the following `#web-split` nodes.

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

Tests: `bun test src` and `bun run cypress:component` (specs mount with `cy.mountWithClients` and port doubles).

Documentation: [docs/README.md](docs/README.md).
