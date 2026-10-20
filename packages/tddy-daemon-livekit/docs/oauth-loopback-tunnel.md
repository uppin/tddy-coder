# OAuth loopback tunnel (tddy-daemon-livekit)

## Overview

When **`livekit.common_room`** is configured, the daemon runs **`oauth_loopback_tunnel`**: it uses the daemon’s **common-room** LiveKit [`Room`] handle, watches **`daemon-*`** participants for **`codex_oauth`** metadata (`pending`, **`authorize_url`**, **`callback_port`**), opens the authorize URL in the system browser, binds **`127.0.0.1:{callback_port}`** with **`tokio::net::TcpListener`**, and bridges each accepted TCP connection to the session host via **`LoopbackTunnelService.StreamBytes`** (**`TunnelChunk`**, first chunk sets **`open_port`**). This replaces the former **tddy-desktop** **`Bun.listen`** + **`@livekit/rtc-node`** path.

## Wiring

- **`run_oauth_tunnel_supervisor_follow_room_slot`** follows the same **`Arc<Room>`** slot as **`tddy_daemon_livekit::livekit_peer_discovery`** so the supervisor (re)starts after common-room connect and reconnect.
- **`spawn_oauth_loopback_tunnel`** starts that supervisor on the shared slot. The module lives in the LiveKit plugin because it *is* LiveKit work — it follows room participants and pipes bytes to one over a room RPC — so `tddy-daemon-auth` names no LiveKit crate. The **`codex_oauth_participant_metadata`** parser it reads (`parse_codex_oauth_metadata`, `resolved_codex_oauth_callback_port`, default port 1455) is a private module beside it; nothing else uses it. The daemon's `runtime.rs` starts the tunnel (through `tddy-session-lifecycle`'s re-export) and composes it with the discovery loop, `spawn_common_room_discovery_task`.
- **Target selection:** **`pick_daemon_oauth_target`** scans remote participants whose identity starts with **`daemon-`** and picks the first with pending **`codex_oauth`** metadata (mirrors desktop **`codex-oauth-metadata`** semantics). **`RpcClient::new_shared`** targets that participant identity for **`loopback_tunnel.LoopbackTunnelService` / `StreamBytes`**.

## Tests

Module tests live in **`oauth_loopback_tunnel.rs`** (metadata pick helpers) and **`codex_oauth_participant_metadata.rs`** (the metadata shape). Full LiveKit coverage remains in **`tddy-livekit`** **`rpc_scenarios`** when a testkit is available.

```bash
cargo test -p tddy-daemon-livekit oauth
```

## Logging

The module logs under target **`tddy_daemon::oauth_tunnel`**, which names the crate the code came from rather than the one it is in. That is deliberate: a log target is an operator's `RUST_LOG` filter, and renaming it would silently stop every filter already selecting it.

## Feature documentation

- **[Codex OAuth relay (daemon)](../../../docs/ft/daemon/codex-oauth-relay.md)** — product context
- **[LiveKit common-room peer discovery](../../../docs/ft/daemon/livekit-peer-discovery.md)** — shared **`Room`** lifecycle
- **[Tddy desktop (Electrobun)](../../../docs/ft/desktop/tddy-desktop-electrobun.md)** — desktop no longer binds OAuth TCP in production

## Related

- **[livekit-service.md](./livekit-service.md)** — the crate this module belongs to
- **[codex-oauth-relay.md](../../tddy-daemon-auth/docs/codex-oauth-relay.md)** — the validation and capture half, in `tddy-daemon-auth`
- **[`session-room.md`](./session-room.md)** — the per-session room in the same crate
- **[changesets/](./changesets/)**
