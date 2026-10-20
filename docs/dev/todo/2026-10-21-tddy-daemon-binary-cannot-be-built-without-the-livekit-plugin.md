# 2026-10-21 — The `tddy-daemon` binary cannot be built without the LiveKit plugin

**Category:** Future enhancement
**Source:** `#satellite` 13/13 (PR #635), PRD `livekit-satellite-lifecycle-launch` § Out of Scope

After `#satellite` no daemon library crate names a LiveKit crate: the ratchet
(`packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs`) allows them only in `PLUGINS` and
the wiring crates. A host with every port at its null object serves local sessions
(`packages/tddy-session-lifecycle/tests/host_without_integrations.rs`).

What is still missing is the switch: `tddy-daemon` depends on `tddy-daemon-livekit` unconditionally,
and `runtime.rs` wires it unconditionally, so a build without LiveKit is not possible.

**What would close it:** make `tddy-daemon-livekit` (and `tddy-livekit`) optional dependencies of
`tddy-daemon` behind a default-on `livekit` cargo feature; put the plugin wiring in `runtime.rs`
(`with_livekit_room_ports`, the reach, the terminal exposure, the split rooms, the common-room
discovery loop, the room-token minter) behind it, with the ports' null objects in the `not(feature)`
arm; add a CI job that builds `--no-default-features`.
