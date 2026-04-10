# tddy-desktop

Native **Electrobun** shell for Tddy: embedded **`tddy-web`**, local Codex OAuth callback listener, and **LiveKit** relay to **`tddy-coder`**.

## Quick start

```bash
# From repo root (nix dev shell)
bun install
bun run --cwd packages/tddy-livekit-web build
bun run --filter tddy-desktop dev
```

- **Dev UI**: set `VITE_URL` (e.g. `http://localhost:5173`) so the webview loads the Vite app.
- **Production UI**: copy `packages/tddy-web/dist/*` into `resources/web/` before `electrobun build`.
- **OAuth relay** (optional): set `TDDY_RPC_BASE` (e.g. `http://127.0.0.1:8899/rpc`), `TDDY_LIVEKIT_URL`, and `TDDY_LIVEKIT_ROOM` to match your daemon session.

## Architecture

Main process (Bun) opens a `BrowserWindow`, optionally runs `runLiveKitOAuthRelay` to join the room, watch `daemon-*` metadata, serve `/auth/callback`, and call `codex_oauth.CodexOAuthService/DeliverCallback` over the existing LiveKit data-channel RPC envelope.

## Documentation

- Product: [docs/ft/desktop/tddy-desktop-electrobun.md](../../docs/ft/desktop/tddy-desktop-electrobun.md)
- Changeset: [docs/dev/1-WIP/2026-04-10-tddy-desktop-electrobun-changeset.md](../../docs/dev/1-WIP/2026-04-10-tddy-desktop-electrobun-changeset.md)
