# 2026-03-21 — Daemon + LiveKit: wait for feature input

- **tddy-coder `--daemon` + LiveKit**: New sessions no longer use a placeholder `"feature"` prompt, which skipped **Feature input** and jumped straight into plan / first clarification. The workflow now blocks until feature text is submitted from the Virtual TUI (browser terminal over LiveKit), matching headless stdin (`/dev/null` from the spawner).
