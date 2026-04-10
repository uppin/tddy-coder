# PRD: Tddy Desktop (Electrobun) — Phases 1–3 Implementation

## Summary

Implement the Tddy Desktop application as an Electrobun-based native shell that embeds `tddy-web`, discovers Codex OAuth authorization requests via LiveKit participant metadata, and relays OAuth callbacks from the local machine to the remote `tddy-coder` agent host over LiveKit.

This PRD covers **Phases 1–3** from the [design document](./tddy-desktop-electrobun.md):
1. **Shell**: Electrobun app window loading `tddy-web` from disk or Vite dev server.
2. **OAuth discovery**: Read `codex_oauth` metadata from LiveKit participants; open system browser for authorization; local callback HTTP server stub.
3. **Relay MVP**: Full OAuth callback relay — desktop captures callback, sends payload via LiveKit RPC to `tddy-coder`, which proxies it to Codex's loopback listener (Variant A).

## Background & Motivation

- **Remote agent problem**: Codex binds its OAuth callback on the agent host's loopback. A developer's laptop browser cannot reach it. Current workarounds (SSH tunnels, device code, copying `auth.json`) are fragile.
- **UX improvement**: A dedicated desktop window with deep-link support is better than juggling browser tabs, Vite ports, and daemon URLs.
- **Relay architecture**: The desktop app completes the browser leg of OAuth on the laptop and relays the callback payload to `tddy-coder` via the existing LiveKit room, where `tddy-coder` proxies it to Codex's local listener.

## Affected Features

| Document | Impact |
|----------|--------|
| [tddy-desktop-electrobun.md](./tddy-desktop-electrobun.md) | Primary design — this PRD implements it |
| `packages/tddy-livekit` | New RPC service method for OAuth callback relay |
| `packages/tddy-coder` | Publish `codex_oauth` metadata; receive relay; proxy to Codex loopback |
| `packages/tddy-web` | Parse `codex_oauth` metadata in ParticipantList (parity with desktop) |
| `packages/tddy-daemon` | Create `codex_oauth_relay` validation helpers referenced in design |

## Requirements

### Phase 1: Desktop Shell

- R1.1: Electrobun application in `packages/tddy-desktop` with `package.json`, build scripts
- R1.2: Main process creates a native window with embedded webview
- R1.3: Production mode: load `tddy-web` from `file://` (bundled `dist/`)
- R1.4: Dev mode: load from Vite dev server URL (`VITE_URL` env var)
- R1.5: Add `packages/tddy-desktop` to root `package.json` workspaces

### Phase 2: OAuth Discovery

- R2.1: Desktop joins LiveKit room (identity `desktop-<session>`) via existing token service
- R2.2: Subscribe to participant metadata changes; parse `codex_oauth` JSON field
- R2.3: When `codex_oauth.pending` is true, open `authorize_url` in system browser
- R2.4: Start local HTTP server on callback port (from metadata or fixed port)
- R2.5: Capture `GET /auth/callback?code=...&state=...` requests; validate state parameter

### Phase 3: Relay MVP (Variant A)

- R3.1: New protobuf RPC service method `CodexOAuthCallback` in `tddy-rpc` envelope
- R3.2: Desktop sends captured callback payload via LiveKit RPC to `tddy-coder` participant
- R3.3: `tddy-coder` publishes `codex_oauth` metadata (`pending`, `authorize_url`, `callback_port`)
- R3.4: `tddy-coder` receives `CodexOAuthCallback` RPC and proxies HTTP to Codex loopback
- R3.5: Security: callback query treated as secret, sent only on authenticated LiveKit data channel, destination restricted to session daemon participant, never logged

### Cross-cutting

- R4.1: Create missing docs (`codex-oauth-web-relay.md`, `codex-oauth-relay.md`)
- R4.2: Create `codex_oauth_relay` validation helpers in appropriate package
- R4.3: Unit tests for all business logic (OAuth parsing, message handling, validation)
- R4.4: E2e tests for desktop app behavior (window creation, callback server, relay flow)

## Success Criteria

- [ ] `packages/tddy-desktop` builds and launches an Electrobun window showing `tddy-web`
- [ ] Desktop app joins LiveKit room and displays participant list with OAuth status
- [ ] Opening an OAuth authorization link in system browser works from desktop
- [ ] Local callback server captures OAuth redirect and sends payload via LiveKit
- [ ] `tddy-coder` receives relay payload and proxies to Codex loopback successfully
- [ ] All unit and integration tests pass
- [ ] Security constraints enforced (no logging of auth codes, channel-restricted delivery)

## Non-goals (this cycle)

- Phase 4: Polish (installer, code signing, auto-update, tray icon)
- Replacing the browser-based dashboard
- Bundling `tddy-daemon` or `tddy-coder` inside the app
- Storing long-lived OpenAI tokens in the desktop app
- Variant B (IPC/hook) relay — documented but deferred

## References

- [Design document](./tddy-desktop-electrobun.md)
- [Electrobun docs](https://electrobun.dev/docs/)
- [LiveKit data channels](https://docs.livekit.io/home/client/data/overview/)
