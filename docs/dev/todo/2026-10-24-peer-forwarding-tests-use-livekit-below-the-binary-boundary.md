# 2026-10-24 — Peer-forwarding tests reach for LiveKit below the binary boundary

**Category:** Future enhancement (test architecture)
**Source:** `#start-progress` 4/8 (split-stream), PR #737

Daemon-to-daemon calls go through a transport-neutral RPC port: `PeerForwarder` (`tddy-daemon-extensions`) —
`call_unary` and `call_server_stream` to a named peer — wrapped by the kernel's `CommonRoom`, which owns the
forwarding policy (unary deadline, stream relay, idle timeout, peer-answered vs peer-silent). LiveKit is one
adapter of that port (`LiveKitPeerForwarder`), not part of the contract.

Yet LiveKit leaks below the binary boundary:

- `tddy-daemon-livekit` re-exports the kernel's typed forwards under historical `*_via_livekit` names
  (`livekit_peer_discovery.rs` ~1382), so callers name a transport they do not depend on.
- Cross-daemon behaviour (split starts, forwarded StreamStartSession, attachment staging across hosts) is proved by
  `tddy-daemon/tests/*_cross_host_acceptance.rs`, which need the LiveKit testkit container — so a forwarding
  change can only be verified end to end on CI, and every such suite pays a container and a full `tddy-daemon`
  build.

**Wanted:** multi-daemon integration tests run over a fully in-memory adapter of `PeerForwarder` — no sockets, no
network, no container: two daemons' service stacks wired through an in-process `PeerForwarder` (request bytes in,
response/stream frames out over channels), so a multi-daemon integration test runs in a single test process. The
in-memory scripted peers already in `tddy-daemon-kernel/tests/common_room_forwarding.rs` and
`tddy-session-split/tests/split_room_hosting_port.rs` are the seed, to be promoted to a shared testkit crate.
Cross-host acceptance suites move onto it; only the binary-level e2e suites use a real transport (LiveKit). Drop the
`*_via_livekit` re-exports in favour of `tddy_daemon_kernel::peer_calls`.

**Why deferred:** a test-architecture change across `tddy-daemon`, `tddy-daemon-livekit` and the session crates,
orthogonal to #start-progress's behaviour; node 4 only removed the one unary re-export it made dead.
