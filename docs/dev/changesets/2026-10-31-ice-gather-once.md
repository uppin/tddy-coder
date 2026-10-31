# 2026-10-31 — ICE candidates gathered once on tddy LiveKit connections

**Type:** Bug Fix

The installed `tddy-daemon` exhausted its 1024 fd soft limit after ~20 h of uptime and
failed every open for over a day — `Projects unavailable: read
~/.tddy/projects/projects.yaml: Too many open files (os error 24)`, with the same EMFILE
taking down `known-hosts.json` writes, `worktree_measurement` git spawns and the web
listener's `accept`. The cause was upstream: tddy connects with `RoomOptions::default()`,
and both the livekit crate (0.7.47) and the libwebrtc binding (0.3.37) default
`rtc_config.continual_gathering_policy` to `GatherContinually`, so every network change
(DHCP lease renewals every ~27 min on this host's two physical interfaces, docker veth
create/destroy churn) re-gathers ICE candidates and the superseded host-candidate sockets
are leaked — the same failure reported upstream as [livekit/rust-sdks#1512]. Measured on
the affected host: ~1000 leaked UDP/TCP-LISTEN candidate sockets spread across the whole
ephemeral range over 46 h of uptime.

Fixed by `tddy-livekit::tddy_room_options()` — the SDK default with gathering pinned to
`GatherOnce`, covered by a unit test asserting the policy — used by every tddy-owned
connect: daemon common-room discovery, the common-room roster joiner, session room joins,
terminal exposure, the screenshare bridge and streamer, the screen-capture streamer, the
coder CLI participant host and `connect_client` (tool RPC clients). Test-only connects
keep the SDK default. A network change that breaks an established connection is handled as
before by the SDK's reconnect flow, which gathers fresh candidates on the fresh connection.
The livekit crate was not upgraded — the upstream fix is not in a release reachable without
a 0.7 → 0.10 major jump; revisit when that upgrade is otherwise due. (tddy-livekit,
tddy-daemon-livekit, tddy-screenshare, tddy-livekit-screen-capture, tddy-coder-cli)

[livekit/rust-sdks#1512]: https://github.com/livekit/rust-sdks/issues/1512
