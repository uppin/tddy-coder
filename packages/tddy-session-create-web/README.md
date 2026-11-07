# tddy-session-create-web

The create-session experience: `CreateSessionPane`, `CreateSessionDialog`, every field and section,
their hooks, the request builder and recipes, codebase placement, attachment staging (`attachments/`)
and start progress.

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

Tests: `bun test src` and `bun run cypress:component` (specs mount with `cy.mountWithClients`).
