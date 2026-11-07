# Changeset: LiveKit wiring package (`tddy-livekit-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 5 — several row-4 signals: three packages (new `tddy-livekit-web`, ports added to `tddy-client-web`, ~31 `tddy-web` files rewired), two new port contracts (presence, media), LiveKit adapter seam, room/track lifecycle ordering

**Stack**: `#web-split` 6/16 — branch `feature/web-split/livekit`, base `feature/web-split/clients`

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

- [ ] Presence + media ports and doubles in `tddy-client-web`
- [ ] `tddy-livekit-web` package: provider, common room, session room, room terminal feed, presence/media implementations, LiveKit screens
- [ ] 31 `tddy-web` importers moved or switched to ports
- [ ] LiveKit specs + support moved; feature specs use port doubles
- [ ] Workspace rule: only the LiveKit packages import `livekit-client`
- [ ] `web-tddy-livekit-web.yml`

## Technical Changes

### State A (Current)

31 non-test files in `tddy-web` import `livekit-client` or the transport: `index.tsx`; `components/ParticipantVideoPreviewDialog.tsx`; `chat/{useAgentChat,AgentChat,useAcpSession}`; `sessions/{SessionScreenSharingTab,useScreenSharingInput,SessionVncTab,SessionUsageTab,VncOverlay,sessionMainPaneProps,useSessionsDrawerSessionList,SessionInspectorDrawer,useSessionUsage,usePresenterLiveKitRoom,ScreenSharingOverlay}`; `sessions/prstack/PrStackChat`; `hooks/{participantCameraVideo,useRoomParticipants,useCommonRoom}`; `session/useCodeSession`; `lib/liveKitConnectionErrors`; `rpc/{selectedDaemon,transportProvider}`; `rpc/connections/{liveKit,livekit/roomTerminalFeed,livekit/sessionConnection,livekit/useDirectRoomTerminal}`; `rpc/hostDirectory/{presenceRoom,useHostPresence,liveKitSource}`. `localHost.ts` carries `liveKitIsConfigured`.

### State B (Target)

`grep livekit-client` outside `packages/tddy-livekit-{web,rpc-web}` finds nothing. `tddy-web/src/index.tsx` calls `livekitPorts({ url, enabled, … })` and passes the result to `ClientsProvider` and the presence/media providers. Feature code reads presence and media through ports.

### Delta

- New package; `git mv` of the LiveKit modules and specs; ports added to `tddy-client-web`; feature files rewritten to port hooks; `vite.config.ts`/`tsconfig` aliases.

## Implementation Milestones

- [ ] Rule test green
- [ ] LiveKit specs green from `tddy-livekit-web`'s config
- [ ] Feature specs in `tddy-web` that touched LiveKit green with doubles (scoped list in pass 2)

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
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`scripts/livekit-import-boundary.test.ts`; `tddy-livekit-web` Cypress; `tddy-client-web` port tests; `tddy-web` scoped specs) — verify 100% pass
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
