# 2026-10-04 — The peer-forward deadline is a daemon setting

**Type:** Feature

`#e2e-leg` 2/5 ([#579](https://github.com/uppin/tddy-coder/pull/579)); cross-package entry:
[2026-10-04-e2e-leg-peer-forward-deadline.md](../../../../docs/dev/changesets/2026-10-04-e2e-leg-peer-forward-deadline.md).

`PeerRouting` holds a `CommonRoom` instead of the bare room slot, built by `PeerRouting::new` from its
own `DaemonConfig` (the signature still takes the slot); `common_room_slot` returns it. The forwarding
call sites in `livekit_peer_discovery` and `peer_routing` moved onto it. Two integration tests
(`forward_to_peer_shared_registry`, `forwarded_rpc_is_stamped_by_the_receiver`) were edited for the API
change with their assertions unchanged. `livekit_peer_discovery.rs` production lines 1,638 -> 1,636
(`oversized-file-livekit-peer-discovery` stays open, row added).
