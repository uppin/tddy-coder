# 2026-08-14 — `StreamLiveKitRooms` on `ConnectionService`

**Type:** Feature

`LiveKitRoomsEvent` is a oneof of `LiveKitRoomsSnapshot` (first message only) and `LiveKitRoomsChange` (every message after it), the latter a oneof of six single-delta events: `room_added` (carrying the full `LiveKitRoomInfo`, participants included, so a consumer never infers a room from a partial event), `room_removed`, `participant_joined`, `participant_left`, `participant_metadata_changed` and `participant_state_changed` — metadata and state are separate facts, so a participant republishing both on one tick produces two events rather than one combined frame that would hide one behind the other. `LiveKitRoomInfo` carries the room's own `metadata` (relayed verbatim from the server API) so a `label` can name an otherwise opaque room; a participant's count is its participant list's length, never a separate field the two could disagree on. (tddy-service)
