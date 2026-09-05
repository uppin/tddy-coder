# 2026-04-11 — Codex OAuth watch on LiveKit reconnect

**Type:** Feature

**`run_with_reconnect`**, **`run_with_reconnect_metadata`**, and **`connect_for_reconnect`** take **`codex_oauth_watch: Option<PathBuf>`**; **`LiveKitParticipant::run`** polls **`try_publish_codex_oauth_metadata`** when set. Feature **[livekit-project-data-ownership.md](../../../../docs/ft/daemon/livekit-project-data-ownership.md)**. (tddy-livekit)
