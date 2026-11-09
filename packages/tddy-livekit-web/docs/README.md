# tddy-livekit-web — technical reference

LiveKit for the tddy web app: the connection provider, the common room, presence, room-addressed
RPC and media (screen sharing and VNC input, the presenter room, camera video), behind
`tddy-client-web`'s ports. With `tddy-livekit-rpc-web` (the transport), it is the only package that
imports `livekit-client` — `scripts/livekit-import-boundary.test.ts` enforces that across the
workspace, with no exceptions. Product description: [livekit-wiring-package.md](../../../docs/ft/web/livekit-wiring-package.md).

## Layout

| Path | Contents |
|---|---|
| `src/livekitPorts.tsx` | `livekitPorts()` — the LiveKit `PresencePort`, `RoomRpcPort` and `MediaPort`, built once — and `LiveKitPortsProvider` |
| `src/media/` | `livekitMedia` (the `MediaPort`): `usePresenterRoom` (+ `presenterRoomTarget`), `useScreenSharingInput` (+ the pure `screenSharingInput` mapping), `useHasCameraVideo` (+ `participantCameraVideo`), `RemoteVideo`, `ParticipantVideoPreview` / `ParticipantVideoPreviewDialog` |
| `src/roomHandle.ts` | `asRoom` / `asHandle`: the only place a `RoomHandle` is said to be a `livekit-client` `Room` |
| `src/transport/liveKitTransport.tsx` | `createDefaultLiveKitTransport`, `LiveKitTransportProvider`, `useLiveKitTransportFactory`, `useLiveKitTransportFactoryIsOverridden` |
| `src/connections/` | `LiveKitConnectionProvider` / `LiveKitConnections`, the session room connection, `roomTerminalFeed`, `useDirectRoomTerminal` |
| `src/rooms/` | `useCommonRoom`, `useRoomParticipants`, `useLiveKitRooms`, `presenceIdentity`, connection-error and status presentation, rooms-panel state and metadata helpers |
| `src/presence/` | `HostPresenceRoom` / `usePresenceRoom`, `useHostPresence`, the common-room directory source (`liveKitSource`) |
| `src/components/` | `LiveKitRoomsContent` (the `#/livekit` body), `LiveKitRoomsPanel`, `ParticipantList` |
| `src/testkit/` | `aJoinedRoom`, `aDisconnectedRoom`, `RoomConnectionState`, `aFakeCommonRoom`, common-room connection fakes (`./testkit` export) |

Exports: `.`, `./testkit`. It imports nothing from `tddy-web` (`src/packageBoundary.test.ts`).

## Registration

`tddy-web`'s `AppClientsPorts` mounts `LiveKitPortsProvider` with ports built once at module scope
(hooks on a port must keep one identity). `RpcTransportProvider` mounts `LiveKitTransportProvider` with
the app's factory — the default transport wrapped in the auth gate and traffic meter; a spec's
`liveKitFactory` override marks it `overridden`, which lets `useRoomClient` build a client before a
room exists (the production default needs a joined room).

`LiveKitConnectionProvider` takes the app's host-served session opener (`openHostServedSession`) as a
fourth constructor argument, so the connection registry and that opener stay in `tddy-web`.

## Room RPC

- `useRoomClient(room, identity, service)` — memoised per factory, room, identity and service; `null`
  until both are known (or a double is in scope).
- `useParticipantPresent(room, identity)` — re-renders on `ParticipantConnected`,
  `ParticipantDisconnected`, `Reconnected`. For rendering.
- `useParticipantProbe(room, identity)` — a stable function reading `room.remoteParticipants` when
  called. Chat's send refusal uses it, so a presenter that left after the last render is still refused.
- `useRoomStatus(room)` — `connected` or `connecting` (a never-joined SDK room reports `Disconnected`,
  read as still joining, as `useCommonRoom` does); `idle` without a room.

## Media

- `usePresenterRoom(hint)` — joins the presenter room a session's attachment hint names; `idle` with
  no room when the hint names none.
- `useScreenSharingInput({ picture, room, bridgeIdentity, framebufferWidth, framebufferHeight,
  onCloseChord })` — forwards pointer and keyboard input on `picture` to the bridge over the
  screen-sharing input stream; `inputUnavailable` once there is no stream to send on (the peer does
  not serve it, or it ended).
- `useHasCameraVideo(room, identity)` — whether the participant has a renderable camera track.
- `RemoteVideo` — the `<video>` for one track, rendered before the track arrives so `videoRef` (the
  input `picture`) is set from the first frame; `testId` keeps the overlays' ids.
- `VideoPreview` — the roster's camera dialog, given the participant's camera track.

`ParticipantList` takes the `room` and asks the port per row. Feature components in `tddy-web`
(`ScreenSharingOverlay`, `VncOverlay`, the session tabs, `HostDesktopOverlay`, the PR-stack and
workflow chat screens) hold a `RoomHandle` and reach media only through `useMediaPort()`.
Diagnostics: `tddy:livekit:participant-video`, `tddy:livekit:screen-input`.

## Tests

`bun test src` (ports, connection provider, session connection, room terminal feed, rooms helpers,
boundary rule, the media mappings) and `bun run cypress:component` (`ParticipantList`, camera capability
gating, the video preview; the roster specs mount through `cypress/support/cameraRoster.tsx`). Neither
needs a LiveKit server, so its `package.json` declares no `tddyCi.needs`.

## Changesets

See [changesets/](changesets/).
