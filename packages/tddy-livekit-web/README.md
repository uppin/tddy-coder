# tddy-livekit-web

LiveKit for the tddy web app. This package and `tddy-livekit-rpc-web` are the only ones that import
`livekit-client`; every other web package reaches a room through `tddy-client-web`'s ports.

- **Registration** — `livekitPorts()` builds the LiveKit implementations of the presence and room-RPC
  ports once; `LiveKitPortsProvider` mounts them. That is the one call `tddy-web` makes.
- **Connection** — `LiveKitConnectionProvider` / `LiveKitConnections` (the `ConnectionProvider` that
  reaches a host over the common room), the session room connection, the room terminal feed and
  `useDirectRoomTerminal`.
- **Rooms and presence** — `useCommonRoom`, `useRoomParticipants`, the common-room directory source,
  `HostPresenceRoom` / `useHostPresence`, `presenceIdentityForUser`.
- **Screens** — `LiveKitRoomsContent` (the `#/livekit` body: `ParticipantList` + `LiveKitRoomsPanel`).
- **Transport seam** — `LiveKitTransportProvider`: the app lends the factory that wraps every LiveKit
  transport (auth gate, traffic meter); this package supplies the default (`createDefaultLiveKitTransport`).
- **Testkit** — `tddy-livekit-web/testkit`: rooms a spec can seed (`aJoinedRoom`, `aDisconnectedRoom`,
  `aFakeCommonRoom`) and rooms that cannot be joined. They return opaque `RoomHandle`s, so a spec
  holding one imports nothing from `livekit-client`.

Not here yet: screen sharing, VNC, the presenter room and camera video — the media layer — which
still sit in `tddy-web` until `feature/web-split/media` moves them behind `MediaPort`.

It imports nothing from `tddy-web`.

Tests: `bun test src` (hooks, session connection, boundary rule) and `bun run cypress:component`
(`ParticipantList`, camera capability gating). Neither needs a LiveKit server.
