# 2026-04-04 — Daemon URL routes: `/terminal/{sessionId}`, SPA fallback, standalone cleanup

- **tddy-web**: **`appRoutes`** helpers (`/terminal/:id`, `/`, `/auth/callback`). **`ConnectionScreen`** (daemon mode) **pushes** the terminal path after Start/Connect/Resume, **replaces** with `/` on Disconnect, handles **popstate** for Back, deep-link attach on load, and unknown-session UI. **`App`** (standalone) **replaces** a stray **`/terminal/...`** URL with **`/`** so standalone keeps the query-param connect model.
- **tddy-coder**: **`web_bundle_acceptance`** asserts **`GET /terminal/...`** returns the SPA **`index.html`** (same stack as **`serve_web_bundle`** SPA fallback).
- **Feature docs**: [web-terminal.md](../web-terminal.md) (URL routes — daemon mode).
