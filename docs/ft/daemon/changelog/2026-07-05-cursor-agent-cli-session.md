# 2026-07-05 — Cursor Agent CLI session

- **`session_type = "cursor-cli"`** — web **Create session** pane, RPC start/resume/connect, gRPC terminal I/O (same path as claude-cli), per-worktree **`.cursor/hooks.json`** → `ReportSessionStatus`, curated model catalog via **`ListAgentModels("cursor-cli")`**, Telegram **`/start-cursor`**. Sandbox and **`WaitingForInput`** are out of scope for v1. Feature: [cursor-cli-session.md](../cursor-cli-session.md).
