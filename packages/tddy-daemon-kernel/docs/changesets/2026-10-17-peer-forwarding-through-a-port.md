# 2026-10-17 — CommonRoom forwards through a PeerForwarder

**Type:** Architecture

`peer_forwarding::CommonRoom` holds an `Arc<dyn PeerForwarder>` and the deadline, built with
`CommonRoom::new(forwarder, &config)`; `is_connected()` replaces `slot()`. `CommonRoom::from_config`,
`slot()` and the private `peer_client` are gone, and `livekit` and `tddy-livekit` leave the manifest
(`tddy-daemon-extensions` joins it). The forwarding methods keep their signatures and their messages.

The failure paths of the policy are pinned over an in-memory forwarder, with no LiveKit server: a peer's
failure reaching the caller unchanged, a stream that cannot be opened, the stream-open deadline, a
terminal error frame relayed once, an undecodable frame ending the relay, and a dropped consumer
releasing the transport's stream.

The `heavy-dependency-livekit-peer-forwarding` code issue is closed on its measurement: the kernel
reaches no LiveKit crate. See the [cross-package entry](../../../../docs/dev/changesets/2026-10-17-peer-forwarding-through-a-port.md).
