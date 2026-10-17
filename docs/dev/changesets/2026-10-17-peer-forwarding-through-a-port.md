# 2026-10-17 — Peer forwarding goes through an extension port

**Type:** Architecture

Forwarding an RPC to a peer daemon is carried by a `PeerForwarder` port (`tddy-daemon-extensions`)
instead of by the LiveKit SDK inside the daemon kernel. The kernel's `CommonRoom` keeps the policy — the
unary and stream-open deadline, the stream idle timeout, the terminal-frame rule and every error a
caller sees — and `LiveKitPeerForwarder` in `tddy-daemon-livekit` supplies the transport. A daemon with
no transport wired holds `NoPeerForwarding`: no peer is connected, and a forward is a
`FailedPrecondition`.

Behaviour is unchanged for operators and callers. Two call sites that asked whether the common room
was up (`tddy-session-agents`, `tddy-session-split`) call `CommonRoom::is_connected()`.

**Final measurement** (closing the `heavy-dependency-livekit-peer-forwarding` code issue on
`tddy-daemon-kernel`, first detected 2026-09-19): `cargo tree -e normal` for `tddy-daemon-kernel` and
for `tddy-host-service` reaches **0** of `livekit`, `livekit-api`, `livekit-protocol` and `tddy-livekit`,
with `tddy-daemon-livekit` as the control that does reach them. The kernel's manifest names none of
them, no kernel source references the SDK (the record counted 1 of 15 modules, previously 304
production lines in `peer_forwarding.rs`), and `peer_forwarding.rs` is 265 production lines. The
ratchet in `tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` agrees.

Per package: [`tddy-daemon-kernel`](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md),
[`tddy-daemon-livekit`](../../../packages/tddy-daemon-livekit/docs/livekit-service.md). Feature:
[LiveKit peer discovery](../../ft/daemon/livekit-peer-discovery.md).
Part of the `#satellite` stack (#624).
