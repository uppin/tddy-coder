# 2026-10-04 — The peer-forward deadline is a daemon setting

**Type:** Feature

`#e2e-leg` 2/5 ([#579](https://github.com/uppin/tddy-coder/pull/579)); cross-package entry:
[2026-10-04-e2e-leg-peer-forward-deadline.md](../../../../docs/dev/changesets/2026-10-04-e2e-leg-peer-forward-deadline.md).

- `DaemonConfig.peer_forward_timeout_secs` (default 30, taken from `PEER_FORWARD_TIMEOUT`) and
  `peer_forward_timeout()` (clamped to 1 s). Three unit tests in `mod peer_forward_timeout_tests`.
- `peer_forwarding::CommonRoom` (room slot plus deadline; `from_config`, `slot`, `forward_timeout`).
  `forward_to_peer`, `forward_to_peer_within` and `forward_server_stream_to_peer` are its methods; the
  free functions and the slot-taking `peer_client` are gone. The stream **open** deadline follows the
  setting; the idle timeout stays `PEER_FORWARD_STREAM_IDLE_TIMEOUT`.
- Production lines: `config.rs` 1,476 -> 1,491, `peer_forwarding.rs` 266 -> 304.
- Code issues: `oversized-file-config` stays open (regressed, +15; recorded, not fixed),
  `heavy-dependency-livekit-peer-forwarding` stays open (unchanged class, still the only SDK consumer).
- Docs: [daemon-kernel.md](../daemon-kernel.md#peer_forward_timeout_secs-and-commonroom--how-long-a-forward-to-a-peer-waits).
