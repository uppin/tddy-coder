# Desktop — changelog

## 2026-04-11 — Bundled tddy-daemon (macOS)

- **tddy-desktop**: **`embedded-daemon`** resolves **`TDDY_DAEMON_CONFIG`** (or **`dev.desktop.yaml`** in dev), loads repo **`.env`**, spawns **`tddy-daemon`** from **`TDDY_DAEMON_BINARY`** / **`resources/bin/`** / **`target/{release,debug}`**; **`prebuild`** builds and copies release binary; **`electrobun.config.ts`** **`build.copy`** includes the binary; teardown on app exit. Feature **[tddy-desktop-electrobun.md](tddy-desktop-electrobun.md)** (bundled daemon section). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-04-11 — OAuth loopback tunnel over LiveKit

- **tddy-desktop**: **`installLiveKitOAuthRelay`** + injected **`startOAuthTcpTunnel`**; raw TCP callback bytes over **`loopback_tunnel.LoopbackTunnelService.StreamBytes`**. **tddy-service**: **`LoopbackTunnelServiceImpl`**. **tddy-coder**: LiveKit **`MultiRpcService`** includes **LoopbackTunnel** (+ **Token** when applicable). **tddy-livekit** / **tddy-livekit-web**: tunnel RPC scenario and **`loopback_tunnel_pb`**. Feature docs: **[tddy-desktop-electrobun.md](tddy-desktop-electrobun.md)**, **[codex-oauth-web-relay.md](../web/codex-oauth-web-relay.md)**, **[codex-oauth-relay.md](../daemon/codex-oauth-relay.md)**. **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-04-10 — Tddy Desktop Electrobun Phases 1–3

- **tddy-desktop**: Electrobun native shell (`packages/tddy-desktop`): embedded `tddy-web` webview, local OAuth callback server, LiveKit relay via `CodexOAuthService/DeliverCallback` RPC (Variant A). Unit, acceptance, and e2e tests. [tddy-desktop-electrobun.md](tddy-desktop-electrobun.md).
- **tddy-service**: `codex_oauth.proto`, `CodexOAuthServiceImpl`, validation helpers (`codex_oauth_validate.rs`), scan module (`codex_oauth_scan.rs`).
- **tddy-livekit**: `spawn_local_participant_metadata_watcher`, `run_with_reconnect_metadata` for metadata watch across reconnection cycles.
- **tddy-coder**: Multi-service wiring (Terminal + Codex OAuth) on LiveKit path, metadata publishing from OAuth detector.
- **tddy-codegen**: Conditional `Stream`/`StreamExt`/`mpsc` imports for unary-only services.
- **tddy-livekit-web**: Generated `codex_oauth_pb.ts`.
- **tddy-web**: `parseCodexOAuthPending` in ParticipantList, `codexOauthMetadata` parser, Cypress + Bun tests.

## 2026-04-10

- **Design** — [tddy-desktop-electrobun.md](tddy-desktop-electrobun.md): Electrobun module `packages/tddy-desktop`, local `tddy-web` + Codex OAuth callback, LiveKit relay to `tddy-coder`.
