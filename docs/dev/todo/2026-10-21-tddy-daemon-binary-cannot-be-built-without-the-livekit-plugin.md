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

**Answered for the client binaries, by the opposite choice (2026-10-21, `#satellite` 15/15, #690):**
`tddy-tools` had exactly this shape, a default-on `livekit` feature on the library with
`NoRoomTransport` in the `not(feature)` arm, and nothing in the repo built it
`--no-default-features`. #690 removed the feature: the binary moved to the wiring crate
`tddy-tools-cli`, which binds `LiveKitRoomConnector` unconditionally, and the library names no
LiveKit crate. The three client wiring crates have no `livekit` feature. If this entry is taken
for the daemon, the feature belongs on the daemon's wiring (`tddy-daemon`), and a matching one on a
client `-cli` crate would be a new decision, not a restoration.
