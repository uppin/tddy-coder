# 2026-10-31 — `tddy_room_options` takes the ICE gathering policy

**Type:** Refactor

`connect_client` keeps gathering once — the tool RPC clients have no configuration surface
and the leak matters there — but does so explicitly:
`tddy_room_options(ContinualGatheringPolicy::GatherOnce)`. The parameter is what lets the
daemon-side callers pass [the operator's
choice](../../../docs/dev/changesets/2026-10-31-ice-gathering-policy.md) through, and
`ContinualGatheringPolicy` is re-exported from the crate root so callers need no
`livekit::webrtc` path. A second unit test pins that a configured `GatherContinually`
reaches the built options unchanged.

Cross-package entry: [The ICE gathering policy is an operator choice](../../../../docs/dev/changesets/2026-10-31-ice-gathering-policy.md).
