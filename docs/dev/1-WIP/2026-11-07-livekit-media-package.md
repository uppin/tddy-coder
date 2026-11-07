# Changeset: LiveKit media behind the media port

**Date**: 2026-11-07
**Status**: 🟢 Green (media implemented, scoped gates passing; validate/wrap pending)
**Type**: Architecture Change
**Complexity**: 4 — three packages (`tddy-livekit-web` implementation, `tddy-client-web` port, `tddy-web` media tabs and overlays), a port contract, LiveKit track lifecycle

**Stack**: `#web-split` 11/17 — branch `feature/web-split/media`, base `feature/web-split/chat`

**PR**: [#855](https://github.com/uppin/tddy-coder/pull/855)

## Initial Discovery

[2026-11-07-livekit-media-package-initial-discovery.md](./2026-11-07-livekit-media-package-initial-discovery.md)

## Affected Packages

- **tddy-livekit-web**: media implementations (screen-sharing input forwarding, presenter room, camera video, `RemoteVideo`, participant video preview).
- **tddy-client-web**: `ports/media.tsx` (final `MediaPort` shape), `testkit/media.tsx` (`aMediaDouble`).
- **tddy-web**: `ScreenSharingOverlay`, `VncOverlay`, `SessionScreenSharingTab`, `SessionVncTab`, `SessionUsageTab`, `ParticipantVideoPreviewDialog`, `usePresenterLiveKitRoom`, `useScreenSharingInput`, `hooks/participantCameraVideo` and their specs.
- **scripts**: `livekit-import-boundary.test.ts` turns green here.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-livekit-media-package.md](../../ft/web/1-WIP/PRD-2026-11-07-livekit-media-package.md)

## Summary

Finish LiveKit isolation: the media layer (screen sharing, VNC, presenter room, camera video) moves into `tddy-livekit-web` behind `MediaPort`, and no `tddy-web` feature file imports `livekit-client` any more.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `MediaPort` final shape and its LiveKit implementation; `aMediaDouble`.
- Feature components take a `RoomHandle` and render tracks through `RemoteVideo`; input forwarding through `useScreenSharingInput` from the port.
- Media specs (`ScreenSharing*`, `VncOverlayAcceptance`, `SessionScreenSharingTargetRowsAcceptance`, `SessionVncTargetRowsAcceptance`, `SessionInspectorScreenSharingAcceptance`, `SessionInspectorVncAcceptance`, `ParticipantVideoCapabilityAcceptance`, `PrStackPresenterRoomAcceptance`, `WorkflowChatPresenterRoomAcceptance`) use the media double, or move to `tddy-livekit-web` when LiveKit behaviour is their point.
- `scripts/livekit-import-boundary.test.ts` green.

## Boundaries

- Does not move hosts, sessions or prstack components into packages (their nodes do); it changes how they get media.
- Does not change `PresencePort`, the room-RPC port or the connection provider (`feature/web-split/livekit`).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` | runs `tddy-livekit-web`'s check | change it |
| `n5` clients (#842) | `tddy-client-web` package and port pattern | adds the final `MediaPort` beside the others | change `ClientsPort` |
| `n6` livekit (#843) | `tddy-livekit-web` package, `RoomHandle`, `PresencePort`, the room-RPC port, `livekitPorts(...)` | registers the media implementation through it | re-create the package or change presence/room-RPC |
| `n7`–`n10` | — (branch predecessors only) | nothing | — |

## Draft PR contract

**As pushed (commit 2, after #843's green f7a3da1):** `aMediaDouble` gains `setCameraVideo` and `presentRoom`
(`TODO(media)`); `src/ports/mediaPort.media.test.tsx` — camera video follows the double, the presenter room the hint
names, pointer input forwarded through the port (3 failing; `RemoteVideo` already renders); the `TODO(media)` allowlist
is deleted from `scripts/livekit-import-boundary.test.ts`, which now lists exactly the 9 media files still importing
`livekit-client`: `ParticipantVideoPreviewDialog`, `ScreenSharingOverlay`, `SessionScreenSharingTab`, `SessionVncTab`,
`VncOverlay`, `usePresenterLiveKitRoom`, `useScreenSharingInput`, `hooks/participantCameraVideo` (+ its test).
`MediaPort`'s interface is unchanged from #843; this node may refine it.

Originally planned:

First push (commit 2): `MediaPort` redrafted against what #843's green settles (`RoomHandle`, room-RPC port), media double signatures, and the media specs re-pointed at the double — failing until the implementation moves; `scripts/livekit-import-boundary.test.ts` (from #843) is the acceptance gate.

## Green wave

**Wave:** 5 of 10
**Greenable independently:** no until #843 is green — it registers through #843's `livekitPorts` and `RoomHandle`
**Concurrent with:** `chat`
**Blocks:** `hosts` (overlays), `session` (media tabs), and the LiveKit rule for every later node

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10 (chat), n11 (media), n14 (prstack)      n11 → n12 (hosts)
    n10 → n13 (workspace), n14      n7 → n14
    n6,n7,n8,n10,n11,n12,n13,n14 → n15 (session)      component packages → n16      all → n17
    waves  1: n1 · 2: n2 n3 n4 · 3: n5 · 4: n6 n7 n8 n9 · 5: n10 n11 · 6: n12 n13 n14 · 7: n15 · 8: n16 · 9: n17

## Scope

- [x] Final `MediaPort` + double
- [x] Media implementations in `tddy-livekit-web`; `tddy-web` media components switched to the port
- [x] Media specs on the double or moved
- [x] `scripts/livekit-import-boundary.test.ts` green

## Technical Changes

### State A (Current)

After #843: `tddy-livekit-web` holds the connection provider, common room, session room, room terminal feed, presence and the room-RPC port; the media files still import `livekit-client`: `ScreenSharingOverlay`, `VncOverlay`, `SessionScreenSharingTab`, `SessionVncTab`, `SessionUsageTab`, `ParticipantVideoPreviewDialog`, `usePresenterLiveKitRoom`, `useScreenSharingInput`, `hooks/participantCameraVideo` (they own `RoomEvent` subscriptions and `VideoTrack.attach`).

### State B (Target)

Those components take `RoomHandle`s and use `MediaPort`; LiveKit track code lives in `tddy-livekit-web`; the import rule is green.

### Delta

- `git mv` of the LiveKit media code into `tddy-livekit-web`; port hooks in the components; spec doubles.

## Implementation Milestones

- [x] Import rule green (no allowlist)
- [x] Media specs green (package and tddy-web, one at a time)

## Testing Plan

**Level:** Cypress component — media double for feature specs, LiveKit fakes inside `tddy-livekit-web`; the workspace import rule.

## Acceptance Tests

### scripts
- `scripts/livekit-import-boundary.test.ts` — `only tddy-livekit-web and tddy-livekit-rpc-web import livekit-client` (from #843; green here).

### tddy-client-web
- `src/ports/mediaPort.test.tsx` — media cases (input forwarding through the port; `RemoteVideo` renders the port's component).

### tddy-web / tddy-livekit-web
- The media specs listed in `## Responsibility` — unchanged assertions.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Split from #843 (developer, 2026-11-07)** after #843's [G] stopped on its own boundary warning: media owns tracks directly and reshapes feature components; connection + presence did not.
- **Placed after `chat`, before `hosts`** so the waves never step back: it is wave 5 (needs #843, wave 4); `hosts` moves to wave 6.

## Green progress (2026-11-07)

**Final `MediaPort` shape** (`tddy-client-web/src/ports/media.tsx`): `usePresenterRoom`, `useScreenSharingInput`,
`useHasCameraVideo`, `RemoteVideo` and — new — `VideoPreview`. `RemoteVideo` gained `testId` and `videoRef` so the
overlays keep their layout and test ids and `picture` for input forwarding is the port's own `<video>`.
`PresencePort` and `RoomRpcPort` are untouched.

**Moved with `git mv` into `tddy-livekit-web/src/media/`**: `ParticipantVideoPreviewDialog`, `useScreenSharingInput`,
`screenSharingInput` (+ test), `usePresenterLiveKitRoom` (now `usePresenterRoom`), `presenterRoomTarget` (+ test),
`participantCameraVideo` (+ test). New: `RemoteVideo`, `useHasCameraVideo`, `ParticipantVideoPreview`, `livekitMedia`
(registered through `livekitPorts().media`). `ParticipantListVideoPreview.cy.tsx` moved to `tddy-livekit-web/cypress`.

**`tddy-web`**: `ScreenSharingOverlay`/`VncOverlay` render `RemoteVideo` and read `useMediaPort()`; the two session tabs
take `RoomHandle`; `WorkflowChatScreen`/`PrStackScreen` call `useMediaPort().usePresenterRoom`; `HostDesktopOverlay`
uses the new `useCommonRoomHandle`; `LiveKitAppPage` no longer lends a dialog.

**#843's `TODO(media)` resolved**: `ParticipantList` lost `participantHasCameraVideo` and `VideoPreview` and its local
affordance-rule copy; it takes `room` and asks the port per row. Its specs mount through `cypress/support/cameraRoster.tsx`
(`aMediaDouble` + `setCameraVideo`).

### Deviations

- `aMediaDouble.usePresenterRoom` reads `hint.room` (the real `SessionAttachmentHint` field); the red test passed
  `{ roomName }` cast `as never`, so that one line in `mediaPort.media.test.tsx` now passes `{ sessionId, room }`.
- `shouldShowParticipantVideoAffordance` was deleted with its one unit assertion ("injected map marks identity as
  having video"): the rule now is `MediaPort.useHasCameraVideo`, asserted by `ParticipantList.cy.tsx` (affordance
  present for `with-cam-user`, absent for `no-cam-user`) and the port tests.
- The roster's `VideoPreview` now receives `room` and attaches the participant's camera track (before, the app lent a
  dialog with no track, so the preview always said "No camera track").
- The input-forwarding, fullscreen, VNC and presenter-room specs stay in `tddy-web` unchanged: they mount through
  `AppClientsPorts`, which now registers the LiveKit media port, and their in-memory LiveKit factory reaches it.
- `RoomRpcPort.useParticipantProbe` (#843's wrap) is not on this branch yet; nothing here touches it.

### Verification (scoped)

`bun test ./scripts/livekit-import-boundary.test.ts ./scripts/web-workflows.test.ts` 4/4; `tddy-client-web` `bun test src` 143/143;
`tddy-livekit-web` `bun test src` 136/136, typecheck clean of new errors; tddy-web `bun run test:unit` 941/941, `bun run build` ok,
tsc 473 errors (474 before; none in code written here). Cypress, one spec at a time: tddy-livekit-web `ParticipantList` 10,
`ParticipantVideoCapabilityAcceptance` 3, `ParticipantListVideoPreview` 3; tddy-web `ScreenSharingFullscreen` 6,
`ScreenSharingInputForwarding` 15, `ScreenSharingInputScopes` 2, `VncOverlay` 4, `SessionScreenSharingTargetRows` 5,
`SessionVncTargetRows` 5, `SessionInspectorScreenSharing` 9, `SessionInspectorVnc` 5, `PrStackPresenterRoom` 2,
`WorkflowChatPresenterRoom` 1, `HostDesktopConnect` 12, `PresenceCapabilityGating` 16. `ScreenSharingInputScopes` failed once on its
first run (0 passing in 87 ms, during bundling) and passed 2/2 on rerun.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-livekit-media-package-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched packages' tests (`scripts/livekit-import-boundary.test.ts`; `tddy-client-web` + `tddy-livekit-web` tests; media specs one at a time) — verify 100% pass
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
