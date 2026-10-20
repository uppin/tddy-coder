# 2026-10-20 — Forwarding consumers off the LiveKit plugin

**Type:** Architecture

`#satellite` 10/13 ([#632](https://github.com/uppin/tddy-coder/pull/632)). Forwarding a typed RPC to a peer, and
routing a request to the daemon that owns it, are transport-neutral code outside the LiveKit plugin. The OAuth
loopback tunnel, which is LiveKit work, moves into the plugin. `tddy-daemon-rpc`, `tddy-session-files` and
`tddy-daemon-auth` leave the ratchet's `KNOWN_OFFENDERS`.

## What changed

- **Typed peer calls.** `tddy_daemon_kernel::peer_calls`: thirteen `forward_*` helpers over `CommonRoom`
  (session, exec-tool, project, session-file families); the unary ones share a private `forward_unary`.
  [Daemon kernel](../../packages/tddy-daemon-kernel/docs/daemon-kernel.md#peer_calls--typed-rpcs-forwarded-to-a-peer).
- **Routing.** `tddy_host_service::peer_routing::PeerRouting::new(config, eligible, Option<CommonRoom>)`, beside
  `EligibleDaemonSource`; the daemon's runtime builds the `CommonRoom` over the LiveKit forwarder.
  [Host service](../../packages/tddy-host-service/docs/host-service.md#peerrouting--which-daemon-serves-an-addressed-request).
- **Consumers.** `tddy-daemon-rpc` (`exec_tool/ports.rs`, `project/coordinate_handlers.rs`, `project.rs`) and
  `tddy-session-files` (`svc_materialize_staged_attachment.rs`, `service.rs`) call the kernel helpers.
- **Tunnel.** `oauth_loopback_tunnel` and its `codex_oauth_participant_metadata` parser move from
  `tddy-daemon-auth` to `tddy-daemon-livekit`; the runtime and `tddy-session-lifecycle`'s re-export follow.
  [OAuth loopback tunnel](../../packages/tddy-daemon-livekit/docs/oauth-loopback-tunnel.md).
- **Token TTL.** `tddy-daemon-auth` spells the six-hour room-token lifetime as `token_provider::ROOM_TOKEN_TTL`
  instead of naming `tddy_livekit::DEFAULT_LIVEKIT_JWT_TTL_SECS`. Same value.
- **Compatibility.** The plugin keeps `forward_*_via_livekit` and `peer_routing` as plain `pub use` re-exports
  (not `#[deprecated]`, which the workspace clippy gate would fail) for the session crates node 11 moves.
- **Ratchet.** Three entries struck. [Extension ports](../../packages/tddy-daemon-extensions/docs/extension-ports.md#the-satellite-rule-and-its-ratchet).

## Code issues

- `oversized-file-livekit-peer-discovery`: 1,700 → 1,407 production lines; seam A done, record narrowed and open.
- `oversized-file-auth`: 619 → 616. `oversized-file-project-coordinate-handlers`: 660 → 652. Both open.

## Not changed

Every forwarded RPC's service, method, deadline and errors; the tunnel's behaviour; `CommonRoom` and the
`PeerForwarder` port; peer discovery.
