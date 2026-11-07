# 2026-11-07 — Package created: connection, presence and room RPC (#web-split 6/17, PR #843)

- New package (the name freed by #841): manifest, tsconfig, vite + Cypress config,
  `cy.mountWithClients`, boundary rule, `web-tddy-livekit-web.yml`, root workspace entry.
- `livekitPorts()` + `LiveKitPortsProvider`: the LiveKit `PresencePort` and `RoomRpcPort`.
- Moved from `tddy-web` with `git mv`: the connection provider and `LiveKitConnections`, session room
  connection, `roomTerminalFeed`, `useDirectRoomTerminal`, `liveKitConnectionErrors`, `useCommonRoom`,
  `useRoomParticipants`, presence (`presenceRoom`, `useHostPresence`, `liveKitSource`),
  `LiveKitRoomsPanel`, `ParticipantList`, `useLiveKitRooms`, the rooms-panel lib helpers,
  `presenceIdentity`, the transport seam (`liveKitTransport`) and its test, and the common-room fakes
  (now `./testkit`). `ParticipantList` and `ParticipantVideoCapabilityAcceptance` specs moved here.
- `LiveKitRoomsContent` is the `#/livekit` body; `tddy-web`'s `LiveKitAppPage` is the shell around it.
- At wrap: `useParticipantProbe` (send-time presence), `statusOf` simplified, unused dependencies
  (`debug`, `tddy-react-commons`, `livekit-server-sdk`, `tddy-rpc-web`) removed, tsconfig includes the
  `debug` ambient declarations of the two packages whose sources it compiles.

Verified: `bun test src` (109), `tsc --noEmit`, Cypress `ParticipantList`,
`ParticipantVideoCapabilityAcceptance`.
