# tddy-livekit-web — technical reference

LiveKit for the tddy web app: the connection provider, the common room, presence and room-addressed
RPC, behind `tddy-client-web`'s ports. With `tddy-livekit-rpc-web` (the transport), it is the only
package that imports `livekit-client` — `scripts/livekit-import-boundary.test.ts` enforces that across
the workspace, with a `MEDIA_NOT_YET_MOVED` allowlist for the media files `feature/web-split/media`
(#855) moves. Product description: [livekit-wiring-package.md](../../../docs/ft/web/livekit-wiring-package.md).

## Layout

| Path | Contents |
|---|---|
| `src/livekitPorts.tsx` | `livekitPorts()` — the LiveKit `PresencePort` and `RoomRpcPort`, built once — and `LiveKitPortsProvider` |
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

## Not here yet

Screen sharing, VNC, the presenter room and camera video (`MediaPort`) — #855. `ParticipantList`
takes the camera dialog as a `VideoPreview` slot with a local copy of the affordance rule until then
(`TODO(media)`); media components in `tddy-web` still type their room prop as `Room` while callers
pass a `RoomHandle`.

## Tests

`bun test src` (ports, connection provider, session connection, room terminal feed, rooms helpers,
boundary rule) and `bun run cypress:component` (`ParticipantList`, camera capability gating). Neither
needs a LiveKit server, so its `package.json` declares no `tddyCi.needs`.

## Changesets

See [changesets/](changesets/).
