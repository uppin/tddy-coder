# 2026-10-20 — Forwarding helpers out, OAuth tunnel in

**Type:** Architecture

`#satellite` 10/13 (#632). The typed `forward_*_via_livekit` helpers and `PeerRouting` left for the kernel and `tddy-host-service`; their old paths are plain re-exports until node 11. `oauth_loopback_tunnel` and its metadata parser moved in from `tddy-daemon-auth` ([oauth-loopback-tunnel.md](../oauth-loopback-tunnel.md)). `livekit_peer_discovery.rs`: 1,700 → 1,407 production lines.

Cross-package entry: [forwarding consumers off the LiveKit plugin](../../../../docs/dev/changesets/2026-10-20-satellite-10-forwarding-consumers.md).
