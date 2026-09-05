# Changeset: optional-livekit-host-directory

**Stack:** `optional-livekit` — node 2 of 7 (parent: `connection-model`, base
`feature/optional-livekit/connection-model`)
PRD: [`2026-09-05-optional-livekit-host-directory-prd.md`](2026-09-05-optional-livekit-host-directory-prd.md)
Discovery: [`2026-09-05-optional-livekit-host-directory-initial-discovery.md`](2026-09-05-optional-livekit-host-directory-initial-discovery.md)

## State A

Inherited from node 1: `HostConnection` + `ConnectionProvider` + registry exist, and every
daemon-level RPC call site resolves its client through them. What remains LiveKit-shaped:

- `SelectedDaemonProvider` -> `useCommonRoomDaemons` -> `useCommonRoom(livekitUrl, commonRoom, identity)`
  constructs and connects a `livekit-client` `Room` and mints a `TokenService` token.
- Hosts are `daemonHostsFromParticipants(useRoomParticipants(room))` (`lib/participantRole.ts`).
- `SelectedDaemonContextValue` still publishes `room: Room | null`, `roomStatus`, `roomError`.
- The three remaining `.room` readers are all **presence**: `LiveKitAppPage:21-22`,
  `RpcPlaygroundAppPage:81-82`, `SessionsDrawerScreen:315-317,856`.
- With no LiveKit config the host list is empty — the serving daemon is not offered, though
  `/api/config` names it.

## State B

- `src/rpc/hostDirectory/` holds `HostDescriptor`, `HostDirectorySource`, the merge, and
  `useHostDirectory()`.
- `LiveKitHostDirectorySource` reproduces today's list; a `ServingHostDirectorySource` contributes
  the daemon serving the page from `servingInstanceId`.
- Unconfigured LiveKit constructs no `Room` and calls no `TokenService` — status `idle`, not `error`.
- `room` is gone from the context. Presence is reached through `useHostPresence(hostId)`, which
  returns `null` unless that host's connection advertises `presence`.
- Selection, persistence, URL sync and the `key={selectedInstanceId}` remount are behaviourally
  unchanged.

## Responsibility

- `src/rpc/hostDirectory/*` — descriptor, source interface, merge, hook.
- `LiveKitHostDirectorySource` and `ServingHostDirectorySource`.
- Reworking `SelectedDaemonProvider` to compose sources; removing `room` from its context value and
  renaming `roomStatus`/`roomError` to directory-level status.
- `useHostPresence(hostId)` and the migration of the three presence consumers onto it.
- Unit tests for merge/de-duplication/status; Cypress component tests for the unconfigured-LiveKit
  case and for a shell driven from an in-memory source.

## Boundaries

- Does **not** define or change `HostConnection`, `ConnectionProvider`, the registry, or
  `useHostClient` — those are node 1's and are consumed as they stand.
- Does **not** touch session-bound connections or `useSessionAttachment`. Node 3 owns those.
- Does **not** hide or gate any media surface. `useHostPresence` returning `null` is a seam node 4
  gates on; this PR leaves the surfaces rendering as they do today.
- Does **not** implement an IPC directory source or register anything from `tddy-desktop`. Nodes 6
  and 7 own those.
- Does **not** change the daemon's common-room advertisement or `/api/config`.
- Adds **no npm dependency**.

## Dependencies

What the parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `connection-model` (#437) | `HostConnection`, `ConnectionCapability`, `ConnectionProvider` and the registry in `src/rpc/connections/*`; `useHostConnection` / `useHostClient`; `LiveKitConnectionProvider`; `useDaemonClient*` re-expressed on them | the directory yields `hostId`s that `useHostConnection` resolves; `useHostPresence` checks the connection's `presence` capability | add, rename or widen anything in `src/rpc/connections/*`; change `useHostClient`'s signature; add a second `ConnectionProvider` |

**Sequencing:** this PR's `useHostPresence` needs `HostConnection.capabilities` to exist. It does as
of node 1's contract commit, so there is no wait.

## Draft PR contract

Lands first, so node 4 can branch off a real ref:

1. `src/rpc/hostDirectory/types.ts` — `HostDescriptor`, `HostDirectorySource`, with final signatures.
2. `src/rpc/hostDirectory/useHostDirectory.ts` — the merge and its hook.
3. `useHostPresence(hostId)` signature.
4. Failing unit tests: merge de-duplicates by `hostId`, per-source status is reported separately,
   an unconfigured source contributes nothing and reports `idle`.
5. Failing Cypress component test: with no `livekitUrl`/`commonRoom`, the selector offers exactly
   the serving daemon and no `Room` is constructed.

Implementation and the removal of `room` from the context land in the same PR under `/green`.
**Not a merge candidate on the contract alone.**

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
- [ ] `/pr-wrap`

## Verification

```bash
./dev bun run --filter tddy-web test:unit
./dev bun run --filter tddy-web cypress:component
```

Installs via `bun run local-registry-install` — the public npm registry is unavailable.

## Successor PRs

- [`2026-09-05-optional-livekit-capability-gating.md`](2026-09-05-optional-livekit-capability-gating.md)
  — branch `feature/optional-livekit/capability-gating`
