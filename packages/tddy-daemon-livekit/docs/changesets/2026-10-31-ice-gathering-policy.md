# 2026-10-31 — The daemon's LiveKit connections follow the configured ICE gathering policy

**Type:** Feature

The four daemon-side connect sites — common-room discovery, the common-room roster joiner,
session room joins and terminal exposure — now pass the operator's
[`livekit.ice_gathering`](../../tddy-daemon-kernel/docs/changesets/2026-10-31-ice-gathering-policy.md)
through [`tddy_room_options`](../../tddy-livekit/docs/changesets/2026-10-31-room-options-takes-policy.md),
mapped from the kernel config enum by `ice_gathering_policy` in the crate root — the one
place the operator's vocabulary becomes the SDK's. The session room's
`LiveKitCredentials::from_config` carries the policy beside the credentials it already
lifts from the same block.

Cross-package entry: [The ICE gathering policy is an operator choice](../../../../docs/dev/changesets/2026-10-31-ice-gathering-policy.md).
