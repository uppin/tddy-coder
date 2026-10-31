# 2026-10-31 — The ICE gathering policy is an operator choice

**Type:** Feature

Follow-up to the same-day gather-once fix, which pinned ICE gathering to `GatherOnce`
because the SDK's `GatherContinually` default leaks the superseded host-candidate sockets on
every network change (livekit/rust-sdks#1512) and exhausted a multi-homed daemon's file
descriptors. That hard-coded the trade-away: continual gathering is also the only mode in
which an established session survives a network switch without a reconnect cycle, and some
deployments prefer that. The policy is now the operator's: `livekit.ice_gathering` in
`daemon.yaml` — `gather_once` (still the default) or `gather_continually` — surfaced as a
select in the web Settings form's LiveKit block, mapped through `LiveKitSettings` in the
daemon config service, which also rebuilds the live common-room connection when the policy
changes (a room joined under one policy does not acquire the other's behaviour by itself).
`livekit.ice_gathering` carries the trade-off in its documentation: choosing
`gather_continually` is an informed acceptance of the fd cost, best paired with a raised
`LimitNOFILE`. `tddy_room_options` takes the policy explicitly; the callers without a
configuration surface — `connect_client` (tool RPC clients), the coder CLI participant
host, and the on-demand screenshare and screen-capture bridges — stay fixed at
`gather_once`, where the leak matters and no operator knob was requested.
(tddy-daemon-kernel, tddy-service, tddy-daemon, tddy-livekit, tddy-daemon-livekit, tddy-web)
