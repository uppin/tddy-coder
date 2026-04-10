# Changesets Applied

Wrapped changeset history for tddy-desktop.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-04-10** [Feature] **Tddy Desktop (Electrobun) Phases 1–3** — Electrobun scaffold (`package.json`, `electrobun.config.ts`), webview loading `tddy-web` from `file://` or `VITE_URL`, `codex-oauth-metadata` parser, `oauth-callback-server` (`/auth/callback`), `livekit-oauth-relay` (`installLiveKitOAuthRelay` → `CodexOAuthService/DeliverCallback` via LiveKit data channel), unit + acceptance + e2e tests. Feature doc: [tddy-desktop-electrobun.md](../../../docs/ft/desktop/tddy-desktop-electrobun.md). (tddy-desktop)
