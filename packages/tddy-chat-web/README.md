# tddy-chat-web

Agent chat: `AgentChat`, the live and transcript views, the ACP session/replay/tool-call hooks,
the agent activity registry and activity panes, and the subagent conversation and permission UI.

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

Tests: `bun test src` and `bun run cypress:component` (specs mount with `cy.mountWithClients`).

Documentation: [docs/README.md](docs/README.md).
