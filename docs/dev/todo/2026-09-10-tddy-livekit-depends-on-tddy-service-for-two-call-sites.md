# 2026-09-10 — `tddy-livekit` depends on `tddy-service` for two call sites, and it costs a crate

**Category:** Future enhancement
**Source:** `unbundle-tools-thinning` changeset (#unbundle node 5, PR #474), M7

`tddy-livekit` has `tddy-service` in `[dependencies]` for exactly two production call sites:

- `packages/tddy-livekit/src/room_roster.rs:13` — `tddy_service::proto::livekit::
  {LiveKitParticipantInfo, LiveKitRoomInfo}`, the mapping into the wire types.
- `packages/tddy-livekit/src/participant.rs:746` —
  `tddy_service::codex_oauth_scan::codex_oauth_from_authorize_url_only`.

Both are *protos and a scanner*, not services. The edge nevertheless makes `tddy-service` an
ancestor of `tddy-livekit`, and that is what forced M7 to create a **twelfth crate**: the
in-session tool client was planned into `tddy-service`, its LiveKit dispatch arm calls
`tddy_livekit::client_connect::connect_client`, and adding that edge yields
`error: cyclic package dependency: package tddy-service depends on itself`. `optional = true`
behind a feature does not exempt it. The result is `packages/tddy-session-tool-client`, a crate
that exists because of a proto placement.

**The fix is one change, and it also closes
[the three `tddy-service` → `tddy-tui` edges](./2026-09-10-three-crates-gained-a-transitive-tddy-tui-dependency.md):
split the generated protos out of `tddy-service` into a crate that depends on nothing.** Then
`tddy-livekit` names the proto crate, the cycle disappears, and `tddy-session-tool-client` could be
folded back into `tddy-service` if that is still wanted.

Deferred because splitting a proto crate out from under `tddy-service`'s ~40 reverse-dependencies
is its own node, and node 5 refuses to widen into one.
