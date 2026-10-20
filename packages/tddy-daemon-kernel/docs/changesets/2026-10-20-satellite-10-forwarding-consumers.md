# 2026-10-20 — Typed peer calls

**Type:** Architecture

`#satellite` 10/13 (#632). `peer_calls`: thirteen typed forwards over `CommonRoom`, moved from the LiveKit plugin under transport-neutral names, tested over a scripted `PeerForwarder` (`tests/peer_calls.rs`). See [daemon-kernel.md](../daemon-kernel.md#peer_calls--typed-rpcs-forwarded-to-a-peer).

Cross-package entry: [forwarding consumers off the LiveKit plugin](../../../../docs/dev/changesets/2026-10-20-satellite-10-forwarding-consumers.md).
