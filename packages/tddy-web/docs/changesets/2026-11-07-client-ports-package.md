# 2026-11-07 — Generated code and client hooks come from `tddy-client-web` (#web-split 5/16, PR #842)

- `src/gen` moved to `tddy-client-web`; every import is now `tddy-client-web/gen/<file>_pb`.
- `rpc/connections/{types,session,terminal}.ts`, the registry class, `useHostFanOut`, `hostLabel` and
  `routing/appLocation` moved; call sites import the hooks from `tddy-client-web`.
- New `rpc/appClientsPorts.tsx` builds the ports from this app's sources; `RpcTransportProvider`,
  `ConnectionProviders`, `HostReadiness`, `SelectedDaemonProvider` mount it (nearest source wins) and
  `AuthProvider` mounts `AuthPortProvider`. `cy.mount` mounts it; `mountWithRpc` delegates to
  `withClients`.
- Kept here: `ConnectionProviders`, `useConnectionProviders`, `HostReadiness`, the hash-navigation store
  (`useAppLocation`, `hashNavigationPort`), `useAuthContext` — components keep using them until the
  nodes that move them.
