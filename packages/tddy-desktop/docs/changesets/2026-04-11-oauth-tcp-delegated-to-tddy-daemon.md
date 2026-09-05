# 2026-04-11 — OAuth TCP delegated to tddy-daemon

**Type:** Architecture

Production **`index.ts`** does not run LiveKit OAuth relay or **`Bun.listen`**; **`TDDY_DESKTOP_OAUTH_RELAY`** warns; **`installLiveKitOAuthRelay`** tests-only. Feature **[tddy-desktop-electrobun.md](../../../../docs/ft/desktop/tddy-desktop-electrobun.md)**; daemon **[oauth-loopback-tunnel.md](../../../../packages/tddy-daemon/docs/oauth-loopback-tunnel.md)**. (tddy-desktop)
