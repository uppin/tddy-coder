# 2026-11-07 — LiveKit connection, presence and room RPC come from `tddy-livekit-web` (#web-split 6/17, PR #843)

- The LiveKit modules listed in `tddy-livekit-web`'s changeset moved there; shared helpers moved to
  `tddy-client-web` (its changeset). Call sites import from those packages.
- `useAgentChat`, `useAcpSession`, `useSessionUsage` build their clients with `useRoomClient` and read
  liveness through `useParticipantProbe` (send-time) and `useRoomStatus`; `AgentChat`,
  `SessionUsageTab`, `PrStackChat`, `useSessionsDrawerSessionList`, `sessionMainPaneProps`,
  `SessionInspectorDrawer`, `selectedDaemon` and `index.tsx` hold a `RoomHandle`.
- `AppClientsPorts` mounts `LiveKitPortsProvider`; `RpcTransportProvider` mounts
  `LiveKitTransportProvider` with the auth-gated, metered factory (its `liveKitFactory` spec seam is
  unchanged). `LiveKitAppPage` is the shell around `LiveKitRoomsContent`.
- `openHostServedSession` and the connection registry stay here and are handed to
  `LiveKitConnections`; `hostServedSession.test.ts` pins the opener (host's own client, rpc only,
  status read through, close latches).
- Specs: the 35 `livekit-client` importers use `tddy-livekit-web/testkit`; the two camera-dialog
  scenarios stay as `ParticipantListVideoPreview.cy.tsx`.
- Still here until #855: screen sharing, VNC, the presenter room and camera video, and the
  `livekit-client` dependency they need.
