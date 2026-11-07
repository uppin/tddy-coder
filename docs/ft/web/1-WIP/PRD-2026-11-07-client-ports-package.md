# RPC interfaces and app ports (`tddy-client-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 5/16 — branch `feature/web-split/clients`

## Affected Features

- **Primary**: [daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md) and the host
  connection model documented in `packages/tddy-web/docs/host-connections.md` — the transport-agnostic
  part moves into a package.
- **Related**: [url-state-routing.md](../url-state-routing.md) (components read and change the app
  location through a port), sign-in flows ([app-shell.md](../app-shell.md)) — components read the
  auth state through a port.

## Summary

Create `packages/tddy-client-web`: everything a component package may know about the outside world.

- **Generated RPC code** — every `*_pb.ts` and connect service descriptor (`tddy-web/src/gen` and its
  buf config), the "RPC interfaces".
- **`ClientsPort`** — how a component gets a client: for the selected daemon, for a named host, over
  plain HTTP, and for a session. Built on the existing provider-agnostic connection model
  (`HostConnection`, `ConnectionProvider`, the provider registry), so HTTP, Tauri IPC and LiveKit are
  implementations supplied by the app, not something a component imports.
- **`NavigationPort`** — read and change the app location (today `routing/useAppLocation`).
- **`AuthPort`** — the signed-in state and its actions (today `hooks/useAuth`), read by ~19 components.
- The consumer hooks keep their names (`useDaemonClient`, `useHostClient`, `useHttpClient`,
  `useSelectedDaemon`, `useDaemons`, `useHostConnection`, `useAppLocation`, `useAuth`, …) and are
  backed by the ports, so ~250 call sites change only their import path.
- A **Cypress testkit** entry: `mountWithClients(jsx, backend)` — every package's component specs
  mount against an in-memory RPC backend through the port, replacing `tddy-web`'s `mountWithRpc`.

## Background

A component package must not depend on `tddy-web`, and must not depend on LiveKit. Today the only way
to get a client is through `tddy-web`'s providers, two of which (`RpcTransportProvider`,
`SelectedDaemonProvider`) import `livekit-client`. Ports declared with the framework
(`feature/web-split/react-commons`) let each package say what it needs; the app supplies it.

## Proposed Changes

### What's Changing

- New package with the generated code, the three ports, the hooks, a port double and the testkit.
- `tddy-web` provides the ports (its current HTTP/Tauri/LiveKit wiring becomes the implementation)
  and imports generated code and hooks from the package.
- The codegen drift gate (`scripts/generated-code.sh`) generates into the new package.
- Boundary rule: no `livekit-client`, no `tddy-web` import.

### What's Staying the Same

- Every RPC, every routing behaviour and every sign-in behaviour is unchanged. LiveKit still works —
  its provider still lives in `tddy-web` until `feature/web-split/livekit`.

## Impact Analysis

### Technical Impact

- ~26k lines of generated code move; ~250 hook call sites and ~300 `gen/` imports change path.
- Cypress specs keep `mountWithRpc` working as a thin alias over `mountWithClients` until each spec
  moves to its package.

### User Impact

None.

## Acceptance Criteria

- [ ] Generated code, ports, hooks and testkit live in `tddy-client-web`; the drift gate passes.
- [ ] A component mounted with `mountWithClients` and an in-memory backend reaches the daemon, a host
      and HTTP services with no `tddy-web` provider in the tree.
- [ ] `tddy-client-web` imports neither `livekit-client` nor `tddy-web`.
- [ ] `tddy-web` behaves as before.

## References

- Predecessor (real): `feature/web-split/react-commons` (ports). Successors: `livekit`,
  `create-session`, `terminal`, `playground` and every later component package.
