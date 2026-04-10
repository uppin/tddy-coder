# Desktop — changelog

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
