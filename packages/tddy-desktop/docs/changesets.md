# Changesets Applied

Wrapped changeset history for tddy-desktop.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-04-11** [Feature] **OAuth loopback tunnel relay** — **`installLiveKitOAuthRelay`** with required **`startOAuthTcpTunnel`** injection; **`loopback_tunnel`** Connect bidi **`StreamBytes`**; tests use mock tunnels. Feature **[tddy-desktop-electrobun.md](../../../docs/ft/desktop/tddy-desktop-electrobun.md)**, **[codex-oauth-web-relay.md](../../../docs/ft/web/codex-oauth-web-relay.md)**. (tddy-desktop)
- **2026-04-11** [Feature] **dev.desktop.yaml + .env for desktop dev** — Repo-root `dev.desktop.yaml` default when `TDDY_DAEMON_CONFIG` unset; `loadRootDotEnv` + `CURRENT_USER` temp config like `./web-dev`; `desktop-dev.sh` loads `.env` and exports default config path. (tddy-desktop)
- **2026-04-11** [Feature] **Embedded tddy-daemon** — Main process spawns `tddy-daemon` when `TDDY_DAEMON_CONFIG` is set; binary from `TDDY_DAEMON_BINARY`, `resources/bin/tddy-daemon` (prebuild), or workspace `target/{release,debug}`; cleanup on exit; `build.copy` + `scripts/build-daemon-for-desktop.sh`. Feature **[tddy-desktop-electrobun.md](../../../docs/ft/desktop/tddy-desktop-electrobun.md)** (bundled daemon section). (tddy-desktop)
- **2026-04-10** [Feature] **Tddy Desktop (Electrobun) Phases 1–3** — Electrobun scaffold (`package.json`, `electrobun.config.ts`), webview loading `tddy-web` from `file://` or `VITE_URL`, `codex-oauth-metadata` parser, `oauth-callback-server` (`/auth/callback`), `livekit-oauth-relay` (`installLiveKitOAuthRelay` → `CodexOAuthService/DeliverCallback` via LiveKit data channel), unit + acceptance + e2e tests. Feature doc: [tddy-desktop-electrobun.md](../../../docs/ft/desktop/tddy-desktop-electrobun.md). (tddy-desktop)
