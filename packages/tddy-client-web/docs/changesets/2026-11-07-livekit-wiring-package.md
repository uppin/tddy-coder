# 2026-11-07 — Presence and room-RPC ports; shared helpers lowered here (#web-split 6/17, PR #843)

- New `src/ports/media.tsx`: opaque `RoomHandle`, `RoomStatus`, `RoomParticipant`; `PresencePort`
  (`useHostPresence`, `usePresenceRoom`, `useRoomParticipants`), `RoomRpcPort` (`useRoomClient`,
  `useParticipantPresent`, `useParticipantProbe`, `useRoomStatus`), and the `MediaPort` declaration
  (its shape is #855's). Each hook throws naming its provider when none is in scope.
- Testkit: `aPresenceDouble`, `aRoomRpcDouble`, minimal `aMediaDouble` (`TODO(media)`, #855).
- Lowered from `tddy-web` with `git mv` (both `tddy-livekit-web` and `tddy-web` need them):
  `participantRole`, `debugMask` (+ `debug.d.ts`; `debug` dependency added), `testId`,
  `terminalFrameContinuity`, `terminalHistoryLoader`, `sessionAttachment`, `useHasCapability`,
  `capabilityAvailability` / `useCapabilityAvailability`, the host directory (`types`, `daemonHost`,
  `useHostDirectory`), `LIVEKIT_SOURCE_ID`, `CommonRoomStatus`. Three of them are slated to move again
  (`docs/dev/todo/2026-11-07-tddy-client-web-holds-ui-and-terminal-helpers.md`).
- At wrap: `useParticipantProbe` added, so chat's send check asks the room at send time rather than
  reading the last rendered presence (the behaviour before the port).

Verified: `bun test src` (129), `tsc --noEmit`.
