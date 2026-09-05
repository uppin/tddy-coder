# 2026-04-10 — Codex OAuth multi-service wiring

**Type:** Feature

LiveKit path registers `CodexOAuthService` alongside `TerminalService` via `MultiRpcService`; `terminal_and_codex_oauth_for_livekit` creates `CodexOAuthServiceImpl` with metadata watch channel; `run_with_reconnect_metadata` receives `Some(metadata_rx)` to push `codex_oauth` JSON to participant metadata. Feature doc: [tddy-desktop-electrobun.md](../../../../docs/ft/desktop/tddy-desktop-electrobun.md). (tddy-coder)
