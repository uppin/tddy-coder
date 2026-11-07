# tddy-prstack-web

The PR-stack experience: `PrStackScreen`, planned-PR list/panel/rows, branch and base-sync status,
reorder and repoint, the start-session modal (opening `tddy-session-create-web`), PR-stack chat (rendering
`tddy-chat-web`) and inspector, and the workflow chat views.

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

Tests: `bun test src` and `bun run cypress:component` (specs mount with `cy.mountWithClients`).
