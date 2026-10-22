# 2026-10-21 — The `tddy-daemon` binary cannot be built without the LiveKit plugin

**Deferred (consent):** 2026-10-22 — user: "defer the remaining", given during `#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)). Not taken by the stack; stays open.
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

## Assessed by `#satellite` 16/16 (the sweep, 2026-10-22): not done there

The sweep's brief allowed this as a fix "if achievable"; it was judged not to be, and not attempted:
`runtime::build` (now `BuildContext`'s stages) names LiveKit types in seven of its eleven files
(`identity`, `integrations`, `mount`, `mount_host`, `mount_entries`, `mount_socket`, `mount_accounts`), as do
`tasks.rs` (`CommonRoomSupervisorTask`, `OauthLoopbackTunnel`), `runtime.rs` (`PeerDiscoveryHandles`) and
`credential_sync.rs` (`livekit::Room`), so a `livekit` feature needs a `not(feature)` arm for each of
them, not one in `runtime.rs`; `tddy-daemon/src/lib.rs` and `livekit_host_ports.rs`, which the feature also
changes, are edited by open PR #694. It is a design of its own, best done on the stages the sweep left.
