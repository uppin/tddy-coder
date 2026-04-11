# Changeset: OAuth loopback tunnel over LiveKit (2026-04-11)

## Plan context (summary)

- **PRD**: `docs/ft/1-WIP/PRD-2026-04-11-oauth-livekit-loopback-tunnel.md`
- **Goal**: Replace operator-desktop HTTP `/auth/callback` parsing + `CodexOAuthService.DeliverCallback` with a **raw TCP proxy** on the operator machine that **tunnels bytes bidirectionally** over LiveKit to `127.0.0.1:{port}` on the session host (Codex listener).
- **Scope**: Tunnel only for this changeset; daemon bundling is out of scope.
- **Acceptance**: Automated test proving bytes round-trip through LiveKit tunnel (plus existing unit test on `LoopbackTunnelServiceImpl`).

## Technical decisions

- **First `TunnelChunk`**: `open_port` connects TCP to `127.0.0.1:open_port`; follow-up chunks carry upstream data; `open_port` on follow-up chunks ignored (warn log).
- **Port guard**: Reject `open_port < 1024` (mitigate dialing privileged ports from tunnel).
- **Legacy removal**: Remove `codex_oauth` unary RPC, `CodexOAuthServiceImpl`, and TS `codex_oauth_pb` from the LiveKit/desktop path. **`CodexOAuthSession` / pending** types live in `codex_oauth_scan.rs` for terminal metadata publishing only.

## Affected packages

- `tddy-service` — tunnel impl, session types move, remove codex_oauth RPC
- `tddy-coder` — LiveKit `MultiRpcService` wiring, `terminal_and_codex_oauth_for_livekit` return type
- `tddy-livekit` — integration test for tunnel over docker LiveKit
- `tddy-livekit-web` — `loopback_tunnel_pb`, exports; drop `codex_oauth_pb`
- `tddy-livekit/proto` — remove `codex_oauth.proto` (keep `loopback_tunnel.proto`)
- `tddy-desktop` — TCP proxy + relay refactor; tests updated

## Milestones

- [x] Changeset authored
- [x] Rust: register tunnel; remove Codex OAuth RPC; port guard (`open_port >= 1024`)
- [x] Buf + TS + desktop TCP proxy + relay refactor
- [x] LiveKit integration: loopback tunnel scenario in `tests/rpc_scenarios.rs` (shared `LiveKitTestkit` lifecycle)
- [x] Docs: `codex-oauth-web-relay.md`, `tddy-desktop-electrobun.md`, desktop README

## Risks

- **Backpressure**: Many small TCP segments → many LiveKit messages; batch if flaky.
- **Security**: Tunnel still allows non-privileged loopback dials; stricter allowlist (metadata port only) is a follow-up.

## Verification (record results when run)

- `./dev cargo test -p tddy-service loopback_tunnel` — **pass** (`stream_bytes_forwards_ping_pong`).
- `./dev cargo check -p tddy-coder` — **pass** (warnings fixed: `_metadata_tx`).
- `./dev cargo test -p tddy-livekit --test rpc_scenarios rpc_scenarios` — includes loopback tunnel scenario; same Docker / WebRTC requirements as other `rpc_scenarios` (see [AGENTS.md](../../AGENTS.md) LiveKit Testkit section).
- `./dev sh -c 'cd packages/tddy-livekit-web && bun run generate && bunx tsc'` — **pass**.
- `./dev bun run --cwd packages/tddy-desktop test` — **pass** (33 tests).
