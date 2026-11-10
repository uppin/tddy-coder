# tddy-session-web

Sessions: the seams session screens reach the app through (`SessionShellProvider` /
`SessionShell`), the session helpers (indicator and notification registry, participant metadata,
labels, grouping, sorting, selection, connection status, traffic meter, transport flavour, LiveKit
ping, the notification feed), the session runtime with its terminals, the main pane, inspector,
agent layers and code pane, and the sessions drawer with project home, the status bar and the traffic
strip — every session surface the app shows. It composes the terminal, chat, hosts, workspace, PR-stack
and create-session packages.

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

Tests: `bun test src` and `bun run cypress:component` (specs mount with `cy.mountWithClients` and port doubles).

Documentation: [docs/README.md](docs/README.md).
