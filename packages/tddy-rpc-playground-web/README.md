# tddy-rpc-playground-web

The RPC playground screen (`RpcPlaygroundScreen`) and its helpers: reflection-driven service
registry, invocation and protobuf-JSON rendering. The app page that places it in the shell stays in
`tddy-web`.

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

Tests: `bun test src` and `bun run cypress:component` (specs mount with `cy.mountWithClients`).
