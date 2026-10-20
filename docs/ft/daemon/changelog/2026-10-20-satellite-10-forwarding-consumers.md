# 2026-10-20 — Peer forwarding no longer names LiveKit outside the plugin

`#satellite` 10/13. Forwarding a request to a peer daemon behaves exactly as before. The typed forwards and the
routing of an addressed request now belong to the daemon's core, and the operator-side Codex OAuth tunnel to the
LiveKit integration. See [LiveKit peer discovery](../livekit-peer-discovery.md) and
[Codex OAuth relay](../codex-oauth-relay.md).
