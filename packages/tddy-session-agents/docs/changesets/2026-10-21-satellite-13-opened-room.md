# 2026-10-21 — The agent-roster room check reads the reach port's `OpenedRoom`

`#satellite` 13/13 (#635). `opened_session_room::require_opened_room(session_id, Option<OpenedRoom>)`
replaces `require_opened_session_room`; `AgentHostCallbacks::ensure_session_room` answers
`Option<OpenedRoom>`. `agent_host_callbacks` re-exports `tddy_room_client::{RoomConnector,
NoRoomTransport}` so the session host fills `AgentRoster::room_connector` without a dependency of its
own. `tests/opened_room.rs`. See [session-agent roster](../session-agent-roster.md).
