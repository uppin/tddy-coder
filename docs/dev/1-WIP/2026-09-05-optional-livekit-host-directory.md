# Changeset: optional-livekit-host-directory

**Stack:** `optional-livekit` — node 2 of 7 (parent: `connection-model`, base
`feature/optional-livekit/connection-model`)
PR: [#438](https://github.com/uppin/tddy-coder/pull/438)
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

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests — `cypress/component/HostDirectoryAcceptance.cy.tsx`
- [x] Run acceptance tests (verify they fail) — 5/5 failing on `useHostDirectory` / `useHostPresence`
- [x] USER REVIEW — acceptance tests — waived 2026-09-05 (run wave 2 straight through)
- [x] TDD Red — write failing unit/integration tests — `src/rpc/hostDirectory/useHostDirectory.test.ts`
- [x] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap`

## Verification

### Baseline after rebasing onto the parent's contract commit

| Check | Result |
|---|---|
| `bun run --filter tddy-web test:unit` | 971 pass, **6 fail** |

The 6 failures are the **parent's** red tests (`src/rpc/connections/registry.test.ts`), inherited
from #437's contract commit. They are not this node's to fix — `connection-model` makes them pass
under its own `/green`. This node adds 9 more.

`bun run --filter tddy-web cypress:component` was run in full at node 1 (207 specs, 1213/1214, one
pre-existing failure in `SelectedHostUrlStateAcceptance.cy.tsx`). Per-node full sweeps were dropped
for nodes 2–5 by agreement — the branches differ only by added files and added failing tests, no call
site changes — and one full sweep runs at the Step 8 completion gate.

### Red status at the contract commit

| Suite | Result |
|---|---|
| `src/rpc/hostDirectory/useHostDirectory.test.ts` | **9 tests, 9 failing** |
| `cypress/component/HostDirectoryAcceptance.cy.tsx` | **5 tests, 5 failing** |

Every failure is on this node's own `TODO(host-directory)` bodies — `mergeHostDirectory`,
`directoryStatusOf`, `hostsOf`, `useHostDirectory`, `useHostPresence`. None is on the parent's
surface.

### Green status

Rebased onto the parent's post-`/green` tip (`a2d9143c`) before implementing, so the 6 inherited
`src/rpc/connections/registry.test.ts` failures recorded above are gone — `connection-model` made
them pass under its own `/green`. There is no pre-existing red left on this branch.

| Suite | Result |
|---|---|
| `bun run --filter tddy-web test:unit` | **1001 pass, 0 fail** (includes `useHostDirectory.test.ts` 9/9) |
| `cypress/component/HostDirectoryAcceptance.cy.tsx` | **5 pass, 0 fail** |
| `cypress/component/CommonRoomConnectionVisibilityAcceptance.cy.tsx` | **4 pass, 0 fail** |
| `cypress/component/DaemonSelectorLiveUpdatesAcceptance.cy.tsx` | **2 pass, 0 fail** |

A further 72 component specs were swept while implementing; the only failures were the three
adapted below.

#### Three existing specs adapted to State B

All three failed for one reason: with a serving daemon named, the directory is never empty, so
preconditions that depended on an empty list became unreachable. Each was re-scoped to the
configuration where the common room is the *only* directory contributor, which is the configuration
each was actually about. No assertion was weakened and no Given/When/Then structure changed.

- `CommonRoomConnectionVisibilityAcceptance` — "tells the daemon selector the room is unreachable"
  now mounts with `servedByADaemon: false` (a new option on `mountWithLiveCommonRoom`). A page
  served by a daemon offers that daemon whatever the room does, so it is never empty to explain;
  the incident guard still holds for the case where it is. The 2026-08-13 reason-reporting is
  additionally covered by the two sibling specs, which pass unchanged.
- `DaemonSelectorLiveUpdatesAcceptance` — both tests dropped `servingInstanceId="udoo"`, which
  collided with the room's own `UDOO` and masked the peer under test. The subject (a daemon present
  across `RoomEvent.Reconnected` re-enters the list) is unchanged.

#### Accepted intermediate state

The directory now names the serving daemon, but `## Boundaries` forbids a second
`ConnectionProvider`, so nothing can connect to it until a later node registers one. An operator
whose common room fails therefore sees their own daemon selected with a per-screen "no connection"
state, where before they saw "Common room unreachable". Ship as planned — confirmed 2026-09-05.

### Commands

```bash
./dev bun run --filter tddy-web test:unit
./dev bun run --filter tddy-web cypress:component
```

Installs via `bun run local-registry-install` — the public npm registry is unavailable.

## Successor PRs

- [`2026-09-05-optional-livekit-capability-gating.md`](2026-09-05-optional-livekit-capability-gating.md)
  — branch `feature/optional-livekit/capability-gating`
