# Changeset: optional-livekit-connection-model

**Stack:** `optional-livekit` — node 1 of 7, root (base `master`)
PR: [#437](https://github.com/uppin/tddy-coder/pull/437)
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

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests — `cypress/component/HostConnectionAcceptance.cy.tsx`
- [x] Run acceptance tests (verify they fail) — 6/6 failing at `useHostConnection`/`useHostClient`
- [x] USER REVIEW — acceptance tests — approved 2026-09-05
- [x] TDD Red — write failing unit/integration tests — `src/rpc/connections/registry.test.ts`
- [x] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — fold the model into `docs/ft/web/daemon-selector-livekit-rpc.md`

## Verification

### Baseline recorded before the contract commit (2026-09-05)

| Check | Result |
|---|---|
| `bun run --filter tddy-web test:unit` | **971 pass, 0 fail** across 114 files |
| `bun run --filter tddy-web cypress:component` | **207 specs, 1214 tests, 1213 pass, 1 fail** (the pre-existing failure below) |
| `cargo build -p tddy-tauri-rpc` | clean (unrelated to this node; warmed for node 6) |

`tsc --noEmit` is **not** a gate in this repo and reports 1288 pre-existing errors (121 files cannot
resolve `bun:test`; Cypress specs pass plain object literals where a proto message is expected). It
was still run, and this node's files contribute none of them.

### Pre-existing failures, recorded so `/green` does not mistake them for this node's red

| Spec | Test | Symptom |
|---|---|---|
| `cypress/component/SelectedHostUrlStateAcceptance.cy.tsx` | `choosing a host records it in the URL` | times out after 4s waiting for `[data-testid='daemon-selector-option-laptop-b']`, via `daemonSelectorPage.choose` |

Observed on this branch **before** any of this node's code was added, so it predates the stack. It
sits in the daemon-selector area node 2 rewrites; node 2 should decide whether its rework fixes it or
whether it needs a separate report.

### Red status at the contract commit

| Suite | Result |
|---|---|
| `src/rpc/connections/registry.test.ts` | **6 tests, 6 failing** |
| `cypress/component/HostConnectionAcceptance.cy.tsx` | **6 tests, 6 failing** |

Every failure is on this node's own `TODO(connection-model)` body — `ConnectionProviderRegistry`'s
methods, `useHostConnection`, `useHostClient`. None is on a parent's surface: this node is a root and
has no parent.

### Green status (2026-09-05)

| Suite | Result |
|---|---|
| `bun run --filter tddy-web test:unit` | **977 pass, 0 fail** across 115 files |
| `cypress:component --spec cypress/component/HostConnectionAcceptance.cy.tsx` | **6 pass, 0 fail** |
| `cypress:component` (full) | **208 specs, 1220 tests, 1219 pass, 1 fail** — the pre-existing failure below |

Both red suites are green and no test file was modified. The unit count is the recorded 971 baseline
plus this node's 6.

The one full-suite failure is `GrpcSessionTerminalResume.cy.tsx` →
`opens TAIL on first connect, then FROM_OFFSET with the tracked offset after a blip`
(`expected 0 to equal 1`). It reproduces identically on the clean tree with this node's code stashed,
and passes in isolation — a suite-order dependency that predates the stack, not this node's.

The `SelectedHostUrlStateAcceptance` failure recorded above did **not** reproduce in either the
baseline or the post-change run on this machine; it appears to be flaky rather than deterministic.

`tsc --noEmit` reports 1235 errors before and after, with an empty diff between the two sorted
lists — this node contributes none. It is not a gate in this repo.

### Deviations from the plan, for review

- **`SessionsDrawerScreen` still reads `room` at line 93.** Only its daemon-level `clientForHost` was
  in scope. The remaining reads are `useRoomParticipants` (presence, expected to remain),
  `buildSessionClient`'s session-scoped fallback (node 3) and the `room` prop to `SessionMainPane`
  (nodes 4/5). The State B post-condition is therefore reached for daemon-level RPC only, as scoped.
- **The LiveKit provider registers from `SelectedDaemonProvider`, not the app root alone.** That is
  the component that owns the `Room`, and 135 test files mount it with no `ConnectionProviders`
  ancestor. `LiveKitConnections` reads the registry in scope and re-provides it, so an app-root
  registry still wins on precedence.
- **Registration happens during render**, with notification deferred to a microtask. A `useEffect`
  version regressed first-paint fleet reads (`ModelsCatalogStateAcceptance`,
  `ModelsFanOutLifecycleAcceptance`).
- **Added beyond the published contract:** `ConnectionProviderRegistry.subscribe`/`revision` (a
  `useSyncExternalStore` seam, so a wire coming up later reaches a subtree that would not otherwise
  re-render) and `useHostConnector()`, a call-time resolver for callers that name hosts inside an
  effect or callback.
- **`useAcpSessionOverClient`'s `peer` changed shape** from `{room, identity}` to
  `{name, isServing()}` — a predicate, so each caller reports liveness in its own wire's terms. Both
  callers updated; behaviour identical.
- **`useModelRegistryFanOut`'s `connected` flag** is now `hasDirectory && daemonIds.every(reachable)`,
  where `hasDirectory` comes from the context's `roomStatus` rather than `room`. With zero known
  hosts `every()` is vacuously true, which would have turned "not connected" into "no daemons". Two
  transient edge states differ in wording only — flag if either must be preserved exactly.
- **Nothing unregisters on unmount** — there is no `unregister` in the contract.

### Commands

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
