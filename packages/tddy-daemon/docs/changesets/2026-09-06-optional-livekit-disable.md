# 2026-09-06 — The common-room switch (`livekit.enabled`)

**Type:** Feature

Node 8 of the `optional-livekit` stack ([#449](https://github.com/uppin/tddy-coder/pull/449)).
See the cross-package entry for the whole change:
[docs/dev/changesets/2026-09-06-optional-livekit-disable.md](../../../../docs/dev/changesets/2026-09-06-optional-livekit-disable.md).

`LiveKitConfig::enabled` (serde default `false`) with a single predicate,
`LiveKitConfig::common_room_enabled`, that every common-room decision delegates to instead of
re-deriving "is LiveKit usable" per call site. Switched off: `CommonRoomTarget::from_livekit` yields
no target, `livekit_common_room_connect_strings` refuses so peer discovery assembles nothing and
publishes no advertisement, and both mints refuse a common-room token while leaving session rooms
alone. `common_room_changed` keys on the flag, so saving the toggle disconnects a live room.

The switch governs the common room only. `livekit.api_secret` still signs session tokens, so a
disabled daemon still authenticates its own gated RPCs — the lockout this design exists to avoid, and
the reason `token.TokenService` authenticates before it inspects the requested room.
