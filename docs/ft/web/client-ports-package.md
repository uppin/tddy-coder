# Client ports package (`tddy-client-web`)

**Package:** `packages/tddy-client-web`
**Applies to:** every web component package
**Added:** 2026-11-07 (#web-split 5/16, PR #842)

## Overview

A component package must not depend on `tddy-web` or on LiveKit. `tddy-client-web` is everything such a
package may know about the outside world:

- **Generated RPC code** — every protobuf message and ConnectRPC service descriptor.
- **Clients port** — how a component gets a client: for the selected daemon, a named host, or plain
  HTTP. HTTP, Tauri IPC and LiveKit are implementations the app registers; no component imports one.
- **Navigation port** — read and change the app location.
- **Auth port** — the signed-in state and its actions.
- **Testkit** — mount a component against an in-memory RPC backend with no `tddy-web` provider.

The consumer hooks keep their pre-split names (`useDaemonClient`, `useHostClient`, `useHttpClient`,
`useSelectedDaemon`, `useAppLocation`, `useAuth`, …).

## Behaviour

- Every RPC, routing and sign-in behaviour is unchanged; `tddy-web` supplies the ports from its existing
  wiring.
- A hook used outside its provider fails loudly, naming the provider; there is no silent default.
- The package imports neither `livekit-client` nor `tddy-web` (enforced by a boundary test).

## Acceptance criteria

- [x] Generated code, ports, hooks and testkit live in `tddy-client-web`; the drift gate passes.
- [x] A component mounted with `mountWithClients` and an in-memory backend reaches the daemon, a host and
      HTTP services with no `tddy-web` provider in the tree.
- [x] `tddy-client-web` imports neither `livekit-client` nor `tddy-web`.
- [x] `tddy-web` behaves as before.

Technical reference: [packages/tddy-client-web/docs/README.md](../../../packages/tddy-client-web/docs/README.md).
