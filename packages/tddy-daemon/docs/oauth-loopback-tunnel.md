# OAuth loopback tunnel (operator)

## Overview

When **`livekit.common_room`** is configured, **`tddy-daemon`** runs **`oauth_loopback_tunnel`**: it uses the daemon’s **common-room** LiveKit [`Room`] handle, watches **`daemon-*`** participants for **`codex_oauth`** metadata (`pending`, **`authorize_url`**, **`callback_port`**), opens the authorize URL in the system browser, binds **`127.0.0.1:{callback_port}`** with **`tokio::net::TcpListener`**, and bridges each accepted TCP connection to the session host via **`LoopbackTunnelService.StreamBytes`** (**`TunnelChunk`**, first chunk sets **`open_port`**). This replaces the former **tddy-desktop** **`Bun.listen`** + **`@livekit/rtc-node`** path.

## Wiring

- **`run_oauth_tunnel_supervisor_follow_room_slot`** follows the same **`Arc<Room>`** slot as **`livekit_peer_discovery`** so the supervisor (re)starts after common-room connect and reconnect.
- **Target selection:** **`pick_daemon_oauth_target`** scans remote participants whose identity starts with **`daemon-`** and picks the first with pending **`codex_oauth`** metadata (mirrors desktop **`codex-oauth-metadata`** semantics). **`RpcClient::new_shared`** targets that participant identity for **`loopback_tunnel.LoopbackTunnelService` / `StreamBytes`**.

## Tests

Module tests live in **`oauth_loopback_tunnel.rs`** (metadata pick helpers). Full LiveKit coverage remains in **`tddy-livekit`** **`rpc_scenarios`** when a testkit is available.

## Feature documentation

- **[Codex OAuth relay (daemon)](../../../docs/ft/daemon/codex-oauth-relay.md)** — product context
- **[LiveKit common-room peer discovery](../../../docs/ft/daemon/livekit-peer-discovery.md)** — shared **`Room`** lifecycle
- **[Tddy desktop (Electrobun)](../../../docs/ft/desktop/tddy-desktop-electrobun.md)** — desktop no longer binds OAuth TCP in production
