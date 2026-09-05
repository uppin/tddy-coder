# 2026-04-11 — OAuth loopback listener migrated to tddy-daemon

**Type:** Architecture

Operator **`TcpListener`** + **`StreamBytes`** client in **`oauth_loopback_tunnel`**, common-room metadata scan (**`codex_oauth_participant_metadata`**), supervisor tied to **`livekit_peer_discovery`**; desktop production path without **`Bun.listen`** or LiveKit OAuth client. **[oauth-loopback-tunnel.md](../../../packages/tddy-daemon/docs/oauth-loopback-tunnel.md)**; **[codex-oauth-relay.md](../../ft/daemon/codex-oauth-relay.md)**; **[tddy-desktop-electrobun.md](../../ft/desktop/tddy-desktop-electrobun.md)**; **[daemon/desktop changelogs](../../ft/daemon/changelog/)**. WIP **`docs/dev/1-WIP/2026-04-11-migration-oauth-loopback-listener-to-daemon.md`** removed after wrap. (tddy-daemon, tddy-desktop, docs)
