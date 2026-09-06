# 2026-09-05 — daemon RPC resolves through a host-connection provider registry

**Type:** Architecture

Everything above the serving-daemon transport was spelled in LiveKit's vocabulary: the peer seam was
`liveKitFactory(room, targetIdentity)` and `selectedDaemon.tsx` built every daemon-level client as
`useLiveKitClient(service, room, daemonRpcIdentity(instanceId))`. A caller could not express "a
connection to host H" without holding a `livekit-client` `Room`, so there was nowhere to plug a
second implementation of one.

`src/rpc/connections/` now names it. `HostConnection`, `ConnectionStatus`, `ConnectionCapability`
(`rpc`/`media`/`presence`) and `ConnectionProvider` are the model; `ConnectionProviderRegistry`
resolves a host through its providers in registration order, first match wins, replacing a
re-registered id **in place** so a wire that reconnects neither overtakes nor falls behind the wires
it was ordered against. An unreachable host, an unselected host and an empty registry all answer
`null` rather than throwing — the guard every call site already had. The registry is also an
observable store (`subscribe` / `revision`, read via `useSyncExternalStore`), so a wire that comes up
after first paint reaches a subtree that would not otherwise re-render.

`LiveKitConnectionProvider` is the only provider, advertising `{rpc, media, presence}` over the
existing `liveKitFactory` and the context `Room`, with the same auth gate and traffic meter. It
claims no host without a room. `LiveKitConnections` registers it from render rather than an effect,
because a screen reading its fleet on mount would otherwise record "no connection" against every host
before the wire was offered and keep that answer; the provider is held in a ref so a discarded or
double-invoked render cannot build a second instance that replaces the first and invalidates every
cached connection and client.

`useHostConnection` / `useHostClient` are the call-site API, `useHostConnector` resolves hosts named
inside an effect or callback, and `useDaemonClient` / `useDaemonClientFor` keep their signatures as
thin names over `useHostClient`. All five daemon-level peer-RPC call sites — `useHostFanOut`,
`useModelRegistryFanOut`, `ModelChatDialog`, `ProjectsAppPage` and `SessionsDrawerScreen`'s cross-host
client — build from a connection instead of a `Room`, leaving presence as the only remaining reader
of `useSelectedDaemon().room`.

`useAcpSessionOverClient`'s `peer` becomes `{name, label, isServing()}`: a predicate, so each caller
reports liveness in its own wire's terms — the session presenter watches room participants, the
models chat reads its connection's status — and the refusal message names the party that is not
answering rather than always saying "the presenter".

Not here: the host directory (still common-room participants), session-bound connections, capability
gating, terminal convergence, the IPC transport, and the desktop wiring. `idle` and `error` have no
producer yet, since a failed LiveKit join lands in `Disconnected` and maps to `connecting`.

No proto, no Rust, no new npm dependency. Tests: 992 unit (15 added across the registry's
observability and the LiveKit provider, neither previously covered), 1222 Cypress component
including a daemon-level screen driven with no room in the tree. Technical
[host-connections.md](../host-connections.md), feature
[daemon-selector-livekit-rpc.md](../../../../docs/ft/web/daemon-selector-livekit-rpc.md).
