# tddy-session-web

Sessions. Today: the seams session screens reach the app through (`SessionShellProvider` /
`SessionShell`), and the session helpers — indicator and notification registry, participant metadata,
drawer labels, grouping, sorting, selection and display, connection status, traffic meter, transport
flavour, LiveKit ping, and the session-notification feed. The session runtime, inspector and drawer
components join it in the following `#web-split` nodes.

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

Tests: `bun test src` (helpers, the seam rules, the boundary rule).

Documentation: [docs/README.md](docs/README.md).
