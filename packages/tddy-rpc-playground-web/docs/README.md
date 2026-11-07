# tddy-rpc-playground-web documentation

The RPC playground screen and its helpers:

- `RpcPlaygroundScreen` — presentational: the participant picker, the reflected service/method tree,
  the request editor (builder ⇄ raw JSON over one value), Invoke, and the response or error. Services
  and invocation arrive as props; the selected service and method are URL state, read and written
  through `tddy-client-web`'s navigation port (`useAppLocation`).
- `buildRegistry` / `findMethod` (`src/registry.ts`) — a protobuf `Registry` from reflection
  `FileDescriptorProto`s, and method lookup by service type name.
- `invokeRpc` (`src/invoke.ts`) — calls a reflected method over a raw Connect `Transport`; unary and
  server-streaming responses come back as JSON, a `ConnectError` as `{ code, message }`.
- `protoMessageJson` (`src/protoMessageJson.ts`) — default request JSON, parse/normalize (empty input
  is `{}`), per-field patching for the builder view.

It imports nothing from `tddy-web` and nothing from LiveKit (`src/packageBoundary.test.ts`). The app
page that hosts the screen — `tddy-web`'s `RpcPlaygroundAppPage` (shell, sign-in, presence,
reflection over the selected host's transport) — is wiring and stays in the app.

- **Changeset history**: [changesets/](changesets/)
- **Product description**: [docs/ft/daemon/rpc-playground.md](../../../docs/ft/daemon/rpc-playground.md),
  [docs/ft/web/rpc-playground-package.md](../../../docs/ft/web/rpc-playground-package.md)

## Tests

`bun test src` (registry, invoke, JSON helpers, the boundary rule; fake transports in
`src/test-utils/`) and `bun run cypress:component` — two component specs from the package's own
`cypress.config.ts`, mounted with `cy.mountWithClients` over an in-memory backend and a navigation
double. CI: `.github/workflows/web.yml` (`Web lint` typechecks it; the `Web tests` shards run its
suites).
