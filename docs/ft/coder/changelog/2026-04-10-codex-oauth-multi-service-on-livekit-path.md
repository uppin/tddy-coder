# 2026-04-10 — Codex OAuth multi-service on LiveKit path

- **tddy-coder**: `terminal_and_codex_oauth_for_livekit` wires `CodexOAuthServiceImpl` alongside `TerminalService` via `MultiRpcService`; `run_with_reconnect_metadata` pushes `codex_oauth` JSON to participant metadata via `watch::Receiver<String>`. Feature doc: [tddy-desktop-electrobun.md](../../desktop/tddy-desktop-electrobun.md).
