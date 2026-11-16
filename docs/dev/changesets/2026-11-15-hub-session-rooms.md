# 2026-11-15 — Session rooms over the in-memory hub

**Type:** Refactor

Third node of the LiveKit test split. Session rooms were the second surface with no
transport-neutral seam: the host speaks to them through the `SessionReach` and `SessionRooms`
ports (`tddy-daemon-extensions`), but only the LiveKit implementations existed.
`tddy-room-testkit` now provides hub implementations:

- `HubSessionReach` — `open_now` joins the session's room in the hub under
  `daemon-{instance_id}`, serves the host's `session_services()` there, and bridges the
  terminal the way `LiveKitSessionReach` does; `session_room_entry` answers
  `OpenSessionRoom` with the hub room's name, the `hub://local` URL and the server identity.
- `HubSessionRooms` — the `SessionRooms` port over the hub's hosted set. It holds each hosted
  session's `HubParticipant` for exactly as long as the room is hosted: the conversion caught
  that `open_room` dropping the participant (whose `Drop` leaves the room) opened a room and
  instantly emptied it.

First conversion: `tddy-daemon-rpc`'s `session_room_exec_tool_acceptance` (1 test, green in
~1 s). Its wiring is exactly these two ports, and `tddy-daemon-rpc`'s
`tddy-livekit-testkit` dev-dependency is gone entirely. `REMAINING_LOGIC_SUITES` drops to 10.

The daemon suites wiring the full `with_the_livekit_room_ports` set follow as the hub grows
those surfaces; they stay named in the placement test until then.

Per-package detail is in each package's own entry under
`packages/<pkg>/docs/changesets/2026-11-15-hub-session-rooms.md`.
