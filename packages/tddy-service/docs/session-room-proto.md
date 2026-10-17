# `session_room.proto`

`session_room.SessionRoomService` is the LiveKit integration's own way into a session. It is
registered by the LiveKit plugin and is absent from a daemon that has none wired; the core protocol
(`session.proto`) never learns about rooms.

| RPC | Request | Response |
|---|---|---|
| `OpenSessionRoom` | `session_token`, `session_id` | `livekit_room` (`session-{session_id}`), `livekit_url` (`public_url` when configured, else `url`), `livekit_server_identity` (`daemon-{instance_id}`, the participant serving the session in that room) |

`OpenSessionRoom` opens the session's room, or reuses the one already open, with this daemon in it and
the session's terminal exposed to the room. It fails, naming the room, when LiveKit cannot be
reached: a caller that asked for a room and has none must hear about it rather than be handed
coordinates nothing is serving. The token is the daemon access token, gated like every other daemon
RPC. A session type this daemon does not facilitate, or a session that records no checkout, is
`failed_precondition`; so is a daemon with no LiveKit credentials.

`ConnectSession` is the counterpart for callers that do not need the room: it is local work and
opens no room for a caller that did not arrive over LiveKit. A client that is about to join a
session's room (`tddy-session-sync`, `tddy-tools pty-relay --livekit-url`) asks here first.

Implementation: [LiveKitSessionReach](../../tddy-daemon-livekit/docs/livekit-service.md#livekitsessionreach--opening-the-room-for-a-reached-session).
Product behaviour: [Session rooms](../../../docs/ft/daemon/session-room.md).
