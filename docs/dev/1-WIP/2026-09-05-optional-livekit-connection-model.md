# Changeset: optional-livekit-connection-model

**Stack:** `optional-livekit` — node 1 of 7, root (base `master`)
PRD: [`2026-09-05-optional-livekit-connection-model-prd.md`](2026-09-05-optional-livekit-connection-model-prd.md)
Discovery: [`2026-09-05-optional-livekit-connection-model-initial-discovery.md`](2026-09-05-optional-livekit-connection-model-initial-discovery.md)

## State A

- `src/rpc/transportProvider.tsx` (336 lines) is the transport seam. Its peer half is
  `liveKitFactory: (room: Room, targetIdentity: string, options?) => Transport`, surfaced as
  `useLiveKitTransportFactory`, `useLiveKitTransport`, `useLiveKitClient` and
  `useLiveKitTransportFactoryIsOverridden`. A peer cannot be named without a `livekit-client` `Room`.
- `src/rpc/selectedDaemon.tsx` (335 lines) exposes `room: Room | null` on `SelectedDaemonContextValue`
  and implements `useDaemonClientFor(service, instanceId)` as
  `useLiveKitClient(service, room, daemonRpcIdentity(instanceId))`.
- Eight files read `.room` off that context. Five read it to build a **peer RPC client**:
  `rpc/useHostFanOut.ts:108`, `components/models/useModelRegistryFanOut.ts:207`,
  `components/models/ModelChatDialog.tsx:38`, `components/projects/ProjectsAppPage.tsx:104`,
  `components/sessions/SessionsDrawerScreen.tsx:93,102-109`.
- The serving-daemon transport is already wire-agnostic (`rpc/daemonTransport.ts`,
  `daemonTransportFlavour`), so *this page's own daemon* is not part of the problem.

## State B

- `src/rpc/connections/` holds a transport-neutral connection model: `HostConnection`,
  `ConnectionStatus`, `ConnectionCapability`, `ConnectionProvider`, and a registry delivered through
  React context.
- A `LiveKitConnectionProvider` implements it over the existing `liveKitFactory` + the context
  `Room`, advertising `{"rpc", "media", "presence"}`. Observable behaviour is unchanged.
- `useHostConnection(hostId)` / `useHostClient(service, hostId)` are the call-site API.
  `useDaemonClient` / `useDaemonClientFor` keep their signatures and are implemented on top.
- All five peer-RPC call sites above build their client from a `HostConnection` instead of a `Room`.
  After this PR the only remaining readers of `useSelectedDaemon().room` are **presence** consumers
  (`LiveKitAppPage`, `RpcPlaygroundAppPage`, `SessionsDrawerScreen`'s `useRoomParticipants`) — which
  is what lets node 2 remove `room` from the context without touching RPC code.
- A registry with no provider registered returns `null` connections and throws nothing.

## Responsibility

- The connection model and its capability vocabulary (`src/rpc/connections/*`).
- The provider registry and its React context, including the test-injection seam.
- `LiveKitConnectionProvider` — the first and, in this PR, only provider.
- `useHostConnection` / `useHostClient`, and `useDaemonClient*` re-expressed on them.
- Migration of every **daemon-level RPC** call site off `useSelectedDaemon().room`.
- Unit tests for the model and registry; Cypress component tests proving a daemon-level screen runs
  with an in-memory provider and no LiveKit object in the tree.

## Boundaries

- Does **not** change the host *directory* — who the hosts are still comes from the common room via
  `SelectedDaemonProvider`, and `room` stays on the context for presence consumers. Node 2 owns that.
- Does **not** touch session-bound connections, `useSessionAttachment`, `useSessionLiveKitRoom`,
  `sessionParticipantRpcClient` or `sessionClientCache`. Node 3 owns those.
- Does **not** gate any media or presence surface on capability. Node 4 owns that.
- Does **not** merge the two terminal components. Node 5 owns that.
- Does **not** add an IPC transport, change `tddy-tauri-rpc`/`tddy-tauri-web`, or register anything
  from `tddy-desktop`. Nodes 6 and 7 own those.
- Does **not** remove `useLiveKitTransportFactory` / `useLiveKitClient` — media surfaces still need
  the raw factory until node 4.
- Adds **no npm dependency**.

## Dependencies

This is a root node: it has no parent PRs and consumes nothing from the stack. It branches off
`master` and can merge on its own.

## Draft PR contract

Lands first, so nodes 2 and 3 can branch off a real ref and compile against real signatures:

1. `src/rpc/connections/types.ts` — `ConnectionCapability`, `ConnectionStatus`, `HostConnection`,
   `ConnectionProvider`, with final signatures.
2. `src/rpc/connections/registry.tsx` — `ConnectionProviderRegistry`, its provider component and
   `useHostConnection` / `useHostClient`.
3. Failing unit tests pinning: resolution order across registered providers, `null` for an
   unreachable host, `null` for an empty registry, and client-identity stability across renders.
4. Failing Cypress component test: a daemon-level screen driven entirely through an in-memory
   provider, asserting no `livekit-client` `Room` is constructed.

The implementation of `LiveKitConnectionProvider` and the five call-site migrations land in the same
PR under `/green` — **this PR does not merge on the contract alone.**

## TODO

- [ ] Record initial discovery
- [ ] Create/update PRD documentation
- [ ] Create changeset
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — fold the model into `docs/ft/web/daemon-selector-livekit-rpc.md`

## Verification

Scope to the packages this node touches (`packages/tddy-web` only):

```bash
./dev bun run --filter tddy-web test:unit
./dev bun run --filter tddy-web cypress:component
```

Dependency installs use the local registry — `bun run local-registry-install` — never plain
`bun install`.

## Successor PRs

- [`2026-09-05-optional-livekit-host-directory.md`](2026-09-05-optional-livekit-host-directory.md)
  — branch `feature/optional-livekit/host-directory`
- [`2026-09-05-optional-livekit-session-connection.md`](2026-09-05-optional-livekit-session-connection.md)
  — branch `feature/optional-livekit/session-connection`
