# Host connections (`src/rpc/connections/`)

`tddy-web` reaches three kinds of thing: the daemon that served this page, some *other* daemon host,
and a session running on one of those hosts. Only the first is expressed without naming a wire
(`rpc/daemonTransport.ts` picks same-origin `/rpc` or the host application's IPC bridge).

A **host connection** is the name for the second. It is what a daemon-level call site asks for
instead of naming a LiveKit `Room` and a participant identity, which is what allows a build that
never joins a common room to reach hosts at all.

Feature docs: [Daemon selector + host-connection routing](../../../docs/ft/web/daemon-selector-livekit-rpc.md).
Sessions on a host are [session connections](session-connections.md), opened from a host connection.

## The model

| Type | What it is |
|---|---|
| `HostConnection` | A live connection to one daemon host: `hostId`, `providerId`, `status`, `error`, `capabilities`, `clientFor(service)`, `transport()`, `openSession(sessionId, hint)` |
| `ConnectionProvider` | A wire that can reach hosts — `id`, plus `connectHost(hostId)` returning a connection or `null` |
| `ConnectionProviderRegistry` | The registered providers, in precedence order |
| `ConnectionCapability` | `rpc` \| `media` \| `presence` — what a connection can do *beyond* plain RPC |
| `ConnectionStatus` | `idle` \| `connecting` \| `connected` \| `error` |

Capabilities sit on the **connection**, not on the host, because they are a property of how you are
connected: the same daemon is media-capable over a common room and not over an in-process bridge.

## Resolution

Precedence is **registration order, first match wins**. It is load-bearing exactly once: a host build
registers its own in-process wire ahead of the common room, so it reaches its own daemon without a
round trip through a media server. Expressing that as order rather than as a preference setting is
why there is no `preferIpc` flag anywhere.

Re-registering an id **replaces the entry where it already stands** rather than appending. A wire
that re-registers on reconnect must neither overtake nor fall behind the wires it was ordered
against — otherwise a dropped common room would silently start routing a host build's own daemon the
long way round.

`null` is a normal answer, not a failure:

| Case | Answer |
|---|---|
| No provider claims the host | `null` |
| No provider registered at all | `null` |
| `hostId` is `null` — nothing selected yet | `null` |
| A provider can reach the host but is not connected | a connection whose `status` says so |

The last row is the distinction that matters: collapsing it into `null` is what would make "this
build has no common room" indistinguishable from "no such host". Every call site already guards a
null client, and that guard is what lets a wire be absent.

## The registry is observable

A wire registers itself when it comes up, which may be after a subtree has already painted and
resolved its hosts to `null`. So the registry is also a store — `subscribe` plus a `revision()`
counter — read through `useSyncExternalStore`. Without it, a screen that asked before the room
existed would hold that answer for as long as the tab stayed open.

The notification is deferred to a microtask and coalesced. Registration happens **during render**
(the only moment early enough for the subtree's first paint to resolve its hosts), and a render may
not update other components; by the time the notification lands, most consumers have already
re-rendered and read the same revision, so the update is dropped. What it catches is the subtree that
would not otherwise re-render at all.

`revision()` is a counter rather than the provider array, because the array is mutated in place: a
`useSyncExternalStore` snapshot has to compare equal until something actually changed.

## Hooks

| Hook | Use |
|---|---|
| `useHostConnection(hostId)` | The connection, or `null` |
| `useHostClient(service, hostId)` | A client for one service on that host, or `null` |
| `useHostConnector()` | A resolver for callers that name hosts inside an effect or callback |
| `useConnectionProviders()` | The registry in scope |

`useHostConnector` exists because a fan-out reads a host list that changes and a form addresses the
host an operator just picked; neither can spend a hook per host. Its identity changes exactly when
routing does, so it is the dependency that replaces `[room, liveKitFactory]` in those callers'
dependency arrays.

**Client identity is stable while routing is.** A connection memoises one client per service, and the
hooks memoise the connection, so a consumer keying an effect on the client does not tear its stream
down on every render.

`useDaemonClient` / `useDaemonClientFor` in `rpc/selectedDaemon.tsx` are thin names over
`useHostClient` — a daemon *is* a host, and the screens read in that vocabulary.

## Registration

`tddy-web` imports no provider. `ConnectionProviders` supplies a registry near the app root;
whoever knows a wire registers it. A component rendered with no `ConnectionProviders` above it gets
an empty registry that resolves everything to `null` — a normal state, not an error path.

The empty fallback is **per component instance**, never a module singleton: a shared one would let a
registration in one test leak into the next.

## The LiveKit provider

`LiveKitConnectionProvider` (`connections/liveKit.tsx`) is the only provider, advertising
`{rpc, media, presence}`. It is built over the same `liveKitFactory` the transport seam already
exposes, so a connection carries the same auth gate and traffic meter a directly-built client would.

- **It claims no host without a room.** With the join in flight — or never attempted — there is no
  participant to name.
- **With a room it claims every host asked of it.** Who exists is the host directory's business; a
  daemon absent from the roster yields a connection whose `status` is `connecting`, which is a
  different claim from "no such host".
- **A provider instance is bound to one room.** A new room means a new provider registered over the
  old one, which drops every transport built against the wire that went away.
- **The transport factory is not part of that identity.** `RpcTransportProvider` rebuilds
  `lkFactory` on every render, so the factory goes stale by identity long before it does by
  behaviour; it is swapped into the standing provider (`rebindFactory`) rather than replacing it.

`LiveKitConnections` registers it from render rather than from an effect, because a screen that reads
its fleet on mount would otherwise record "no connection" against every host before the wire was
offered, and keep that answer. Registering from render is only safe if it is idempotent *across
instances*, so the provider is held in a ref: a memo factory may run for a render React then discards
(StrictMode double-invokes; a concurrent render may be thrown away), and each such run would build a
second provider that replaces the first, bump the revision, and invalidate every connection and
client in the app.

Nothing unregisters on unmount. A registration says a wire exists, and the registry it says it into
either goes with the component or outlives the whole daemon-mode session.

## Current limits

- **`idle` and `error` have no producer.** LiveKit has no terminal failed state — a failed join lands
  in `Disconnected`, which maps to `connecting`. `HostConnection.error` is therefore always `null`
  today. A wire with a real failure mode is what makes those states reachable.
- **A connection's capabilities are read fleet-wide in one respect.** Every media and presence
  surface is gated on them (see [capability gating](capability-gating.md)), but the *status* half of
  that rule — is a join in flight, did it fail — comes from the one LiveKit host directory source
  the page holds, not from the host being asked about. That is exact while one wire is registered
  and needs revisiting at the first mixed fleet.
- **The host directory is still the common room.** Who the hosts *are* comes from common-room
  participants via `SelectedDaemonProvider`, so a build with no room resolves no hosts even though
  the connection model itself no longer requires one.
