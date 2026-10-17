# 2026-10-17 — LiveKitPeerForwarder carries forwards to peer daemons

**Type:** Architecture

`LiveKitPeerForwarder` implements the kernel's `PeerForwarder` port over the common-room `Room` slot. It
builds the same `LiveKitRpcClientFactory` client addressed at `daemon-{peer_id}` that the kernel used to
build, keeps that client alive in a relay task for a stream's whole life, and releases it when the
reader drops. `PeerRouting` and the common-room discovery loop build their `CommonRoom` through it.

Its relay channel is 32 frames deep and independent of the transport's own channel; a reader that stops
draining backs the transport up, so a port consumer must drain promptly. The forwarding suites
(`forward_to_peer_shared_registry`, `forwarded_rpc_is_stamped_by_the_receiver`,
`livekit_peer_daemons_acceptance`) change only in how they build a `CommonRoom`.
