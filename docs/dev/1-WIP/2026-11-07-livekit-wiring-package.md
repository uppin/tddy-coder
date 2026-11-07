# Changeset: LiveKit wiring package (`tddy-livekit-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 5 — still several row-4 signals after the 6a/6b split: three packages (new `tddy-livekit-web`, ports in `tddy-client-web`, ~25 `tddy-web` files rewired), two port contracts (presence, room RPC), LiveKit adapter seam, room/track lifecycle ordering

**Stack**: `#web-split` 6/17 — branch `feature/web-split/livekit`, base `feature/web-split/clients`

**PR**: [#843](https://github.com/uppin/tddy-coder/pull/843)

## Initial Discovery

[2026-11-07-livekit-wiring-package-initial-discovery.md](./2026-11-07-livekit-wiring-package-initial-discovery.md)

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) and `packages/*/docs/code-issues/` this change runs into.

### ℹ ANSWERED — `#optional-livekit` evaluation: "Is LiveKit configured?" decided in six places — [`2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation.md`](../todo/2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation.md)

`liveKitIsConfigured` (`rpc/connections/localHost.ts`) and the web `clientConfig` read become one input to `livekitPorts`. That removes the web-side re-derivation; the daemon-side ones the entry lists stay open.

### ⚠ DURING — LiveKit Docker testkit can stall the event loop — [`2026-10-18-livekit-docker-testkit-can-stall-the-event-loop-with-no-deadline.md`](../todo/2026-10-18-livekit-docker-testkit-can-stall-the-event-loop-with-no-deadline.md)

If `tddy-web`'s `livekitDockerTestkit.ts` moves here, its behaviour is unchanged; recorded, not fixed here.

## Affected Packages

- **tddy-livekit-web** (new — the name freed by #841): `packages/tddy-livekit-web/` — README, `docs/`, sources, Cypress specs + LiveKit support, workflow.
- **tddy-client-web**: `ports/presence.ts`, `ports/media.ts` added (owned here, in the package from #842).
- **tddy-web**: [README.md](../../packages/tddy-web/README.md) — the 31 `livekit-client` importers move or switch to ports; `index.tsx` registers the LiveKit provider and port implementations from `tddy-livekit-web`; `docs/` (`host-connections.md`, `capability-gating.md`) at wrap.
- **tddy-livekit-rpc-web**: consumed (transport), unchanged.
- **root** `workspaces`, `bun.lock`; `.github/workflows/web-tddy-livekit-web.yml` (Docker LiveKit + echo fixture inputs, as #838's caller for the transport has).

## Related Feature Documentation

- PRD: [PRD-2026-11-07-livekit-wiring-package.md](../../ft/web/1-WIP/PRD-2026-11-07-livekit-wiring-package.md)

## Summary

Every remaining `livekit-client` use leaves `tddy-web`'s feature code: LiveKit implementations live in `tddy-livekit-web`, features reach them through `ClientsPort`, `PresencePort` and `MediaPort`, and `tddy-web` wires them in.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `PresencePort` (`tddy-client-web/ports/presence`): host presence entries, a presence room handle, `useHostPresence`, `usePresenceRoom` — the shape `rpc/hostDirectory/{presenceRoom,useHostPresence,liveKitSource}` consumers read today, without LiveKit types.
- `MediaPort` (`tddy-client-web/ports/media`): room participants (`useRoomParticipants`), participant camera video, screen-sharing tracks + input forwarding (`useScreenSharingInput`), VNC media, the presenter room (`usePresenterRoom`), session usage media stats — as opaque handles rendered by components the port supplies (`<RemoteVideo track=…/>`), never `livekit-client` objects.
- `tddy-livekit-web`: `LiveKitConnectionProvider` (implements `ConnectionProvider`), `LiveKitConnections`, common room (`useCommonRoom`), session room connection, room terminal feed (`roomTerminalFeed`, `useDirectRoomTerminal`), `liveKitConnectionErrors`, the LiveKit `selectedDaemon` source, the presence and media implementations, `LiveKitRoomsPanel`, `LiveKitAppPage`'s panel content, `ParticipantList`, `ParticipantVideoPreviewDialog`, and `livekitPorts(...)` — the one call `tddy-web` makes to register all of it.
- Cypress: LiveKit support (`fakeCommonRoom`, `commonRoomConnection`, `recordingLiveKitRpc`, `withLiveCommonRoom`, the Docker testkit) and the LiveKit specs (`LiveKitRoomsPanelAcceptance`, `LiveKitScreenAcceptance`, `ParticipantList`, `ParticipantVideoCapabilityAcceptance`, `CommonRoomConnectionVisibilityAcceptance`, `PresenceCapabilityGatingAcceptance`, `useLiveKitTerminalToken`, `DaemonSelectionSurvivesRoomDisconnectAcceptance`).
- A port double for presence and media in `tddy-client-web/testkit`, so feature specs run without a room.
- The workspace rule test `scripts/livekit-import-boundary.test.ts`: only the two LiveKit packages import `livekit-client`.

## Boundaries

- Does not move chat, sessions, prstack, hosts or any feature component — they stay in `tddy-web` and switch from LiveKit objects to ports. Their extraction is later nodes'.
- Does not change the LiveKit RPC transport (`tddy-livekit-rpc-web`, #841) or any daemon behaviour.
- Does not change `ClientsPort` (#842); presence and media are new ports beside it.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` with `needs-rust-fixture` / `needs-livekit-image` inputs | adds `web-tddy-livekit-web.yml` using both | change the shared workflow |
| `n3` ui (#840) | primitives, `cn`, theme | the LiveKit panels render with them | move UI primitives |
| `n4` livekit-rpc (#841) | the transport as `tddy-livekit-rpc-web`; the name `tddy-livekit-web` free | depends on it for RPC over rooms; takes the name | rename or change the transport |
| `n5` clients (#842) | `ClientsPort`, `ConnectionProvider`, `ClientsProvider`, testkit | implements `ConnectionProvider`; adds presence/media ports and doubles to its package | change `ClientsPort` or the hook names |

## Draft PR contract

**Re-planned 2026-11-07 (commit 3) — split 6a/6b.** The first [G] stopped on this changeset's own boundary
warning, before changing anything: `Room` is a prop of ~8 feature components; chat, session usage and
screen-sharing input make **room-bound RPC** (`liveKitFactory(room, identity)`, `room.remoteParticipants.has`,
`ConnectionStateChanged`) that no planned port covered; the media overlays own tracks (`RoomEvent`,
`VideoTrack.attach`); ~35 specs inject Room fakes. The developer chose to split:

- **This node (6a): connection, presence and room RPC.** `tddy-livekit-web` with the connection provider,
  common room, session room connection, room terminal feed, presence (`PresencePort`), `ParticipantList`,
  the rooms panel/screen, and a new **`RoomRpcPort`** (`useRoomClient(room, identity, service)`,
  `useParticipantPresent`, `useRoomStatus`) + `aRoomRpcDouble`. Chat (`useAgentChat`, `useAcpSession`,
  `AgentChat`), `useSessionUsage`/`SessionUsageTab`, `PrStackChat`, `selectedDaemon`, `transportProvider`,
  `useSessionsDrawerSessionList`, `sessionMainPaneProps`, `SessionInspectorDrawer` switch to `RoomHandle` +
  these ports. Their specs swap Room fakes for the doubles.
- **`feature/web-split/media` (#855, 6b):** screen sharing, VNC, presenter room, camera/participant video
  behind `MediaPort` (declared here; #855 owns its final shape).
- `scripts/livekit-import-boundary.test.ts` now carries `MEDIA_NOT_YET_MOVED` — a `TODO(media)` allowlist
  for the media files only. This node must make the rule green **with** that allowlist; #855 empties it.

Contract after commit 3: 7 failing in `tddy-client-web` (4 presence/media + 3 room-RPC) and the narrowed rule.

Earlier contract (commit 2):

**As pushed (commit 2):**

- `tddy-client-web/src/ports/media.tsx` — `RoomHandle` (opaque), `RoomState`, `RoomParticipant`,
  `PresencePort` (`useHostPresence`, `usePresenceRoom`, `useRoomParticipants`), `MediaPort`
  (`usePresenterRoom`, `useScreenSharingInput`, `useHasCameraVideo`, `RemoteVideo`), their providers and
  hooks — bodies `TODO(livekit)`. Ports carry hooks; the app builds each port once, so hook identity is stable.
- `tddy-client-web/src/testkit/media.tsx` — `aPresenceDouble`, `aMediaDouble` (`TODO(livekit)`).
- Failing: `src/ports/mediaPort.test.tsx` (4) and `scripts/livekit-import-boundary.test.ts`, which today
  finds **72** files outside the LiveKit packages importing `livekit-client` — the 31 sources listed in
  State A plus `tddy-web` Cypress specs and support. Specs move to `tddy-livekit-web` or switch to the
  doubles.
- **Not in the contract:** the `tddy-livekit-web` package skeleton — `packages/tddy-livekit-web` is still
  the transport until #841 (rename) is green. Created at green.

Originally planned:

First push (commit 2): `PresencePort` and `MediaPort` types with their hooks' signatures (`// TODO(livekit): implement`), the `tddy-livekit-web` skeleton exporting `livekitPorts` and `LiveKitConnectionProvider` signatures, the moved LiveKit specs pointing at the new package, the port-double specs, and the workspace rule `scripts/livekit-import-boundary.test.ts` — failing while 31 `tddy-web` files still import `livekit-client`.

## Green wave

**Wave:** 4 of 9
**Greenable independently:** yes — its specs use the moved LiveKit fakes and port doubles; n4's transport and n5's ports are published on the base
**Concurrent with:** `create-session`, `terminal`, `playground`
**Blocks:** `chat`, `hosts`, `prstack` (their LiveKit uses must be behind ports first), and through them `workspace`, `session`

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [x] Presence + room-RPC ports and doubles in `tddy-client-web` (media port plumbing; media shape is #855's)
- [x] `tddy-livekit-web` package: provider, common room, session room, room terminal feed, presence implementation, LiveKit screens (media implementation: #855)
- [x] `tddy-web` importers moved or switched to ports (the media files stay, allowlisted)
- [x] LiveKit specs + support moved where their subject is the package; feature specs use the package testkit (see Deviations)
- [x] Workspace rule: only the LiveKit packages import `livekit-client` (media allowlist until #855)
- [x] `web-tddy-livekit-web.yml`

## Technical Changes

### State A (Current)

31 non-test files in `tddy-web` import `livekit-client` or the transport: `index.tsx`; `components/ParticipantVideoPreviewDialog.tsx`; `chat/{useAgentChat,AgentChat,useAcpSession}`; `sessions/{SessionScreenSharingTab,useScreenSharingInput,SessionVncTab,SessionUsageTab,VncOverlay,sessionMainPaneProps,useSessionsDrawerSessionList,SessionInspectorDrawer,useSessionUsage,usePresenterLiveKitRoom,ScreenSharingOverlay}`; `sessions/prstack/PrStackChat`; `hooks/{participantCameraVideo,useRoomParticipants,useCommonRoom}`; `session/useCodeSession`; `lib/liveKitConnectionErrors`; `rpc/{selectedDaemon,transportProvider}`; `rpc/connections/{liveKit,livekit/roomTerminalFeed,livekit/sessionConnection,livekit/useDirectRoomTerminal}`; `rpc/hostDirectory/{presenceRoom,useHostPresence,liveKitSource}`. `localHost.ts` carries `liveKitIsConfigured`.

### State B (Target)

`grep livekit-client` outside `packages/tddy-livekit-{web,rpc-web}` finds nothing. `tddy-web/src/index.tsx` calls `livekitPorts({ url, enabled, … })` and passes the result to `ClientsProvider` and the presence/media providers. Feature code reads presence and media through ports.

### Delta

- New package; `git mv` of the LiveKit modules and specs; ports added to `tddy-client-web`; feature files rewritten to port hooks; `vite.config.ts`/`tsconfig` aliases.

## Implementation Milestones

- [x] Rule test green
- [x] LiveKit specs green from `tddy-livekit-web`'s config
- [x] Feature specs in `tddy-web` that touched LiveKit green (scoped list under Green Progress)

## Testing Plan

**Level:** Cypress component in the package (LiveKit fakes; the Docker testkit only where a spec needs a real server, as today) + `bun test` for the presence/media port hooks against doubles + the workspace rule. Feature specs in `tddy-web` keep their assertions and swap LiveKit fakes for port doubles.

## Acceptance Tests

### scripts

- `scripts/livekit-import-boundary.test.ts` — `only tddy-livekit-web and tddy-livekit-rpc-web import livekit-client`.

### tddy-client-web

- `src/ports/presencePort.test.tsx` — `useHostPresence lists the hosts the port reports`; `a presence change re-renders`.
- `src/ports/mediaPort.test.tsx` — `useRoomParticipants lists the port's participants`; `screen-sharing input forwards through the port`.

### tddy-livekit-web

- Moved: `LiveKitRoomsPanelAcceptance`, `LiveKitScreenAcceptance`, `ParticipantList`, `ParticipantVideoCapabilityAcceptance`, `CommonRoomConnectionVisibilityAcceptance`, `PresenceCapabilityGatingAcceptance`, `useLiveKitTerminalToken`, `DaemonSelectionSurvivesRoomDisconnectAcceptance` (`.cy.tsx`, unchanged assertions).
- `cypress/component/LiveKitPortsAcceptance.cy.tsx` — `livekitPorts registers a provider that reaches a host through the common room`; `presence from the room reaches usePresence`; `with LiveKit off, no provider claims a host`.

## Green Progress (2026-11-07, 6a)

Scope is the 6a re-plan: connection, presence and room RPC. Media is #855's.

- **Ports** (`tddy-client-web/ports/media.tsx`): `PresencePort`, `RoomRpcPort`, `MediaPort` providers and hooks
  (`useRoomClient`, `useParticipantPresent`, `useRoomStatus`, `useHostPresence`, `useRoomParticipants`,
  `useMediaPort`); doubles `aPresenceDouble`, `aRoomRpcDouble`, and a minimal working `aMediaDouble` (TODO(media)).
  `MediaPort` / `ScreenSharingInputOptions` declarations untouched.
- **`tddy-livekit-web`** created (manifest, tsconfig, vite + Cypress config, `cy.mountWithClients`, README,
  `packageBoundary.test.ts`, `web-tddy-livekit-web.yml`, root workspace, `bun.lock`). `livekitPorts()` +
  `LiveKitPortsProvider` is the one registration call; `tddy-web`'s `AppClientsPorts` mounts it, so every
  subtree under any app provider has presence and room RPC.
- **Moved with `git mv`**: the connection provider, session room connection, room terminal feed,
  `useDirectRoomTerminal`, `liveKitConnectionErrors`, `useCommonRoom`, `useRoomParticipants`, presence
  (`presenceRoom`, `useHostPresence`, `liveKitSource`), `LiveKitRoomsPanel`, `ParticipantList`,
  `useLiveKitRooms`, the LiveKit lib helpers (`codexOauthMetadata`, `liveKitMetadataCard`,
  `liveKitRoomsState`, `liveKitStatusPresentation`), `presenceIdentity`, the transport test, and the
  fakes (`fakeCommonRoom`, `commonRoomConnection`) as `tddy-livekit-web/testkit`.
- **Lowered to `tddy-client-web`** (shared by the moved code and by `tddy-web`, so neither package can own
  them): `participantRole`, `debugMask` (+ `debug.d.ts`, `debug` dependency), `testId`,
  `terminalFrameContinuity`, `terminalHistoryLoader`, `sessionAttachment`, `useHasCapability`,
  `capabilityAvailability`/`useCapabilityAvailability`, the host directory (`types`, `daemonHost`,
  `useHostDirectory`), `LIVEKIT_SOURCE_ID`, `CommonRoomStatus`.
- **Rewired in `tddy-web`**: `useAgentChat`, `useAcpSession`, `useSessionUsage` read a client through
  `useRoomClient` and liveness through `useParticipantPresent`/`useRoomStatus`; `AgentChat`,
  `SessionUsageTab`, `PrStackChat`, `useSessionsDrawerSessionList`, `sessionMainPaneProps`,
  `SessionInspectorDrawer`, `selectedDaemon`, `index.tsx` hold `RoomHandle`. `LiveKitAppPage` is the
  app shell around `LiveKitRoomsContent`. `RpcTransportProvider` mounts `LiveKitTransportProvider` with the
  auth-gated, metered factory (so the spec seam `liveKitFactory` still routes everything to a backend).
- **Specs**: the 35 `livekit-client` importers now use `tddy-livekit-web/testkit` (`aDisconnectedRoom`,
  `aJoinedRoom`, `RoomConnectionState`). `ParticipantList` and `ParticipantVideoCapabilityAcceptance` moved
  to the package; the two camera-dialog scenarios stayed in `tddy-web` as `ParticipantListVideoPreview.cy.tsx`.
- **Rule**: `scripts/livekit-import-boundary.test.ts` green with the `MEDIA_NOT_YET_MOVED` allowlist; the 7 port
  tests green.

### Verified (scoped)

- `tddy-client-web`: `bun test src` 128/128, typecheck clean, Cypress 1/1.
- `tddy-livekit-web`: `bun test src` 106/106, typecheck clean, Cypress 13/13 (no LiveKit server needed).
- `tddy-web`: `bun run build` ok; `bun test` over the `test:unit` directories 1141/1141; Cypress (one at a
  time) `LiveKitRoomsPanelAcceptance` 26, `CommonRoomConnectionVisibilityAcceptance` 4,
  `PresenceCapabilityGatingAcceptance` 16, `DaemonSelectionSurvivesRoomDisconnectAcceptance` 1,
  `SessionConnectionAcceptance` 10, `useSessionAttachment` 2, `PrStackChatSilentFailureAcceptance` 5,
  `PrStackChatAcceptance` 4, `AgentActivityClientIdentityAcceptance` 2, `DesktopIpcHostAcceptance` 18
  (one run had 3 failures that did not reproduce in three reruns), `HostConnectionGateAcceptance` 5,
  `CreateSessionStartProgress` 5, `SessionTerminalIdentity` 3, `SessionsDrawerAcceptance` 23,
  `ParticipantListVideoPreview` 3.
- scripts: boundary rule, package-name rule, web-workflows 8/8.

### Deviations

- **Shared helpers lowered to `tddy-client-web`**, not moved into `tddy-livekit-web`: both packages need
  them, and the UI-kit node's rule (helpers move with the first package that consumes them from outside
  `tddy-web`) puts them in the lowest shared package. `tddy-client-web` gains the existing `debug` dependency.
- **Injection instead of moving app wiring**: `openHostServedSession` (the host-served terminal feed) and the
  connection registry stay in `tddy-web` and are handed to `LiveKitConnections`; the transport wrapper is
  lent through `LiveKitTransportProvider`. `HostServedSessionOpener` is a fourth constructor argument of
  `LiveKitConnectionProvider`.
- **Three provider-behaviour scenarios** (host-served session: same client as the host, rpc only) are now
  asserted against the opener stub in `tddy-livekit-web`; the opener's own behaviour has no direct test yet.
  TODO: a `hostServedSession.test.ts` in `tddy-web`.
- **`ParticipantList`'s camera dialog is a slot** (`VideoPreview`) and its affordance rule a local copy,
  because `ParticipantVideoPreviewDialog` and `participantCameraVideo` are media files (#855). TODO(media).
- **Spec placement**: only the specs whose subject is entirely in the package moved. The rest drive
  `tddy-web` screens (`SelectedDaemonProvider`, `SessionRuntime`, `LiveKitAppPage`) and stay, using the
  package's testkit. `useLiveKitTerminalToken.cy.tsx` stays (its subject is a `tddy-web` hook). The Docker
  testkit stays with `tddy-web`'s Cypress config; no spec in the package needs a LiveKit server, so the
  workflow sets neither `needs-livekit-image` nor `needs-rust-fixture`.
- **Test ids**: the participant-list ids exist in the package's `cypress/support/testIds.ts` and still in
  `tddy-web`'s (specs there drive the roster through screens). TODO: collapse when those specs move.
- **`tddy-web` still depends on `livekit-client`** for the media files. TODO(media).
- **Room-bound hooks became event-driven**: `useParticipantPresent` re-renders on join/leave, where chat read
  `room.remoteParticipants` at send time; a send closure now sees the last rendered value.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Complexity 5 — boundary warning.** The node carries two new contracts. If `MediaPort` cannot be made LiveKit-free without reshaping screen-sharing/VNC components, stop and split by capability: connection + presence here, media as its own node before `hosts`.
- **Opaque media handles + port-supplied renderers** keep `MediaStreamTrack`/LiveKit types out of feature packages.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-livekit-wiring-package-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched packages' tests (`scripts/livekit-import-boundary.test.ts`; `tddy-livekit-web` Cypress; `tddy-client-web` port tests; `tddy-web` scoped specs) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
