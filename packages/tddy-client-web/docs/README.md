# tddy-client-web — technical reference

What a web component package may know about the outside world: generated RPC code, the
provider-agnostic connection model, and the ports read through hooks: clients, navigation, auth, and
(since #843) presence, room RPC and media.
Product description: [client-ports-package.md](../../../docs/ft/web/client-ports-package.md).

## Layout

| Path | Contents |
|---|---|
| `src/gen/**` | `buf generate` output (`buf.gen.yaml`, `buf.yaml` live here). Committed; checked by `scripts/generated-code.sh check packages/tddy-client-web` |
| `src/connections/` | `types.ts` (`HostConnection`, `ConnectionProvider`, `ConnectionCapability`, `ConnectionStatus`), `session.ts`, `terminal.ts`, and `registry.ts` — the observable `ConnectionProviderRegistry` (first registration wins; same-id re-registration keeps its place; coalesced microtask notification, `revision()` snapshot; a later wire may lend the winner its `media` — see [Media is lent](#media-is-lent-calls-are-not)) |
| `src/ports/clients.tsx` | `ClientsPort`, `ClientsProvider` and the client hooks |
| `src/ports/navigation.tsx` | `NavigationPort`, `NavigationProvider`, `useAppLocation` |
| `src/ports/auth.tsx` | `AuthPort`, `AuthPortProvider`, `useAuth` |
| `src/appLocation.ts` | the pure `AppLocation` model (parse/format, `withPath`, `withParams`) |
| `src/hostFanOut.ts`, `src/hostLabel.ts` | `useHostFanOut`, `hostLabelOf` |
| `src/ports/media.tsx` | `RoomHandle` (opaque), `RoomStatus`, `RoomParticipant`; `PresencePort`, `RoomRpcPort`, `MediaPort`, their providers and hooks (#843) |
| `src/hostDirectory/` | `DaemonHost` and host-directory types, `daemonHost`, `useHostDirectory` (lowered from `tddy-web` in #843) |
| `src/capability/`, `src/connections/useHasCapability.ts` | `capabilityAvailability` / `useCapabilityAvailability`, `useHasCapability` (#843) |
| `src/connections/sessionAttachment.ts` | `attachmentHintFromReply`, `capabilitiesForHint` (#843) |
| `src/lib/` | `participantRole`, `debugMask` (the `debug` dependency, typed by `@types/debug`), `testId`, `terminalFrameContinuity`, `terminalHistoryLoader` — shared by `tddy-livekit-web` and `tddy-web` (#843; `testId`, `terminalFrameContinuity` and `debugMask` are slated to move, see `docs/dev/todo/2026-11-07-tddy-client-web-holds-ui-and-terminal-helpers.md`) |
| `src/rpcFailure.ts`, `src/randomId.ts`, `src/fileUploadChunks.ts`, `src/worktreeFilesApi.ts` | `rpcFailureText` / `hostNotConnected`, `randomUuid`, chunked upload (`UPLOAD_CHUNK_TIMEOUT_MS`), the worktree file-listing API — lowered from `tddy-web` in #844, shared by `tddy-session-create-web` and `tddy-web` |
| `src/appRoutes.ts`, `src/authFlow.ts`, `src/useActionFailure.ts` | the session route builders (`sessionsDrawerPathForSession`, `sessionsProjectHomeSetupPath`, `sessionsProjectNewPath` / `parseSessionsProjectNew` for the orchestrator composer, `singleSegmentAfter`, `decodeSegment`), `AuthFlow` / `AuthFlowDeclaration`, and the one-surface failure slot `useActionFailure` — lowered from `tddy-web` in #849, shared by `tddy-workspace-web` and `tddy-web` |
| `src/testkit/` | port doubles and `withClients` (`./testkit` export) |

Exports: `.`, `./gen/*`, `./connections/*`, `./testkit`.

## Ports

A port is a plain React context per concern. Each `*Provider` mounts an implementation for a subtree;
each hook throws, naming the provider, when none is in scope. The package carries **no default
implementation** — the app (or a spec's double) supplies every port.

| Port | Shape | Hooks |
|---|---|---|
| `ClientsPort` | `{ httpTransport: Transport; connections: ConnectionProviderRegistry; isHostReady?(hostId); selectedDaemon: SelectedDaemonState }` | `useClientsPort`, `useHttpTransport`, `useHttpClient`, `useConnectionRegistry`, `useHostConnector`, `useHostConnection`, `useHostClient`, `useSelectedDaemon`, `useDaemons`, `useHostLabel`, `useDaemonClient`, `useDaemonClientFor` |
| `NavigationPort` | `{ location(); navigate(to, options?); subscribe(listener) }` | `useNavigationPort`, `useAppLocation` → `{ location, navigate(path), setParams(patch) }` |
| `PresencePort` | `{ useHostPresence(hostId); usePresenceRoom(); useRoomParticipants(room) }` | `useHostPresence`, `usePresenceRoom`, `useRoomParticipants`, `usePresencePort` |
| `RoomRpcPort` | `{ useRoomClient(room, identity, service); useParticipantPresent(room, identity); useParticipantProbe(room, identity); useRoomStatus(room) }` | same names, plus `useRoomRpcPort` |
| `MediaPort` | `{ usePresenterRoom(hint); useScreenSharingInput(options); useReceivedVideoStats(room, identity, trackName); useHasCameraVideo(room, identity); RemoteVideo; VideoPreview }` | `useMediaPort` |
| `AuthPort` | the `useAuth` state (`user`, `isAuthenticated`, `sessionToken`, `vaultState`, `deviceLogin`, …) and actions (`login`, `logout`, `startDeviceLogin`, `unlockVault`, …) | `useAuth` |

**Ports that carry hooks.** `PresencePort` and `RoomRpcPort` members are hooks, so the app builds each
port once (`tddy-livekit-web`'s `livekitPorts()`) and their identities hold for the app's lifetime.
Rooms are opaque `RoomHandle`s; no LiveKit type crosses the port.

**Rendered presence versus a send-time check.** `useParticipantPresent` re-renders on join, leave and
reconnect — for rendering. A check made inside an event handler (chat refusing a send nobody would
read) uses `useParticipantProbe`, a stable function that asks the room *when called*: a value from the
last render would miss a leave or join since then.

The hook names are the ones `tddy-web` used before the split, so a moved component changes only its
import path. `useHostConnector` is `"use no memo"`: its memo is keyed on the registry `revision`, which
the resolver never reads, and the React Compiler would drop that key.

**Why plain contexts and not `tddy-react-commons/ports` `withPorts`.** `withPorts` binds adapters to an
`AppHandler` that a `ReactAppContainer` starts and tears down; its consumers read the handler's
context, not a port. These ports are read by hooks in arbitrary components, per render, and the
nearest provider must win for a subtree (a spec or a nested provider overriding one source). A context
per port is that mechanism with no container lifecycle in between; wrapping it in `withPorts` would add
a handler that does nothing but forward its ports.

## Media is lent, calls are not

`ConnectionProviderRegistry.connectHost(hostId)` asks every provider in registration order; the
first that reaches the host **wins**, and its calls, sessions and status are the host's. The winner
is handed back as it is unless a connection behind it also reaches the host and has a lendable
capability the winner lacks. Then the result is a `BorrowingMediaConnection`: the winner, also
advertising each lent capability for as long as some lender is `connected`.

- **Only `media` is lendable** (`LENDABLE_CAPABILITIES`). The media port joins the room a stream's
  start reply names with a token of its own, so a host whose wire carries only calls can still have
  its screen watched once some wire on the page reaches the media server it publishes into. RPC
  and presence belong to the wire a host is reached over; lending them would route calls or rosters
  where precedence said they must not go.
- **Read live.** The capability set is computed when asked, so a lender's room connecting or its
  peers coming and going changes it without re-resolving the host.
- **Stable identity.** One borrowing connection is memoised per winner and set of lenders, so a
  host resolved twice is the same object while its routing holds (`useHostConnection` and the
  effects keyed on it depend on that).

The case it exists for is Tddy Desktop's own host: its IPC connection advertises `{"rpc"}`, and
when the common room is joined with that daemon on it, the LiveKit connection lends `media`, so
the Hosts screen offers Connect for a desktop on that machine (an iOS Simulator, say). With no
LiveKit, nothing is lent. `src/connections/registry.test.ts` covers lending, live status and
identity.

## How tddy-web supplies the ports

`tddy-web/src/app/rpc/appClientsPorts.tsx` (`AppClientsPorts`) builds `ClientsPort` from the app's
sources — the serving-daemon transport, the connection registry (`ConnectionProviders`), host readiness,
the selected-daemon directory — and mounts `hashNavigationPort` as `NavigationPort`. Each source
provider (`RpcTransportProvider`, `ConnectionProviders`, `HostReadiness`, `SelectedDaemonProvider`)
renders `AppClientsPorts` below itself, and `AuthProvider` mounts `AuthPortProvider`. Nested mounts are
intentional: the innermost port is rebuilt from the **nearest source of each kind**, so it is always a
superset of the outer one, and a spec that mounts only one source provider still gets a coherent port.
A single root mount would freeze the sources seen at the root and miss a nested
`SelectedDaemonProvider` or test override. Without a `SelectedDaemonProvider` the directory source
reports "no daemons". `tddy-web`'s `cy.mount` also mounts `AppClientsPorts`.

The source-side components (`ConnectionProviders`, `useConnectionProviders`, `HostReadiness`) and the
implementations (hash navigation store, sign-in, LiveKit provider) stay in `tddy-web`; component
packages never see them.

## Testkit

`tddy-client-web/testkit` (test code only — nothing under `src/` outside `testkit/` and test files
imports it):

- `aClientsPortDouble(backend, { daemons?, selectedInstanceId? })` — every transport (HTTP and every
  listed host) is the `tddy-connectrpc-testkit` in-memory backend's; unlisted hosts resolve to `null`.
- `aNavigationDouble(initial?)` — in-memory, records `history`.
- `anAuthDouble(stateOverrides?)` → `AuthDouble`: signed in by default; every action is a `Spy` that
  resolves and records `.mock.calls`. The spy is one plain recording function in both `bun test` and
  the Cypress bundle (no runner-specific branch).
- `aPresenceDouble()` — rooms are plain tokens (`room(name)`), `presentHost`, `setParticipants`.
- `aRoomRpcDouble(backend)` — every room client is the backend's; `setPresent(room, identity, here)`
  drives both `useParticipantPresent` and probes.
- `aMediaDouble(overrides?)` → `MediaDouble`: `setCameraVideo(room, identity, has)`, `presentRoom(name, state)`;
  `useScreenSharingInput` listens on the real picture and records `forwardedInput` (pointer types, keys),
  calling `onCloseChord` on Ctrl+Alt+Escape; `RemoteVideo` is a bare `<video>`.
- `withClients(element, backend, overrides?)` — wraps in all three ports; `cy.mountWithClients` and
  `tddy-web`'s `mountWithRpc` use it.

## Boundary and checks

`src/packageBoundary.test.ts` (via `scripts/web-package-boundary.ts`) forbids imports of `tddy-web` and
`livekit-client`. CI: `.github/workflows/web.yml` runs `tsc --noEmit` (`Web lint`), `bun test src` and
`cypress/component/MountWithClients.cy.tsx` (`Web tests` shards).

## Changesets

See [changesets/](changesets/).
