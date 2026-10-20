# 2026-10-20 — PeerRouting

**Type:** Architecture

`#satellite` 10/13 (#632). `peer_routing::PeerRouting` moved here from the LiveKit plugin; its constructor takes `Option<CommonRoom>` instead of a LiveKit room slot. See [host-service.md](../host-service.md#peerrouting--which-daemon-serves-an-addressed-request).

Cross-package entry: [forwarding consumers off the LiveKit plugin](../../../../docs/dev/changesets/2026-10-20-satellite-10-forwarding-consumers.md).
