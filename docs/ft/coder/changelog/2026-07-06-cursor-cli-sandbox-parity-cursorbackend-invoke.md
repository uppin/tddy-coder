# 2026-07-06 — Cursor CLI sandbox parity (`CursorBackend::invoke`)

- **`CursorBackend::invoke`** registers `tddy-tools --mcp` + `permission-prompt-tool` for headless tool approvals and exports `TDDY_SOCKET`, `TDDY_REPO_DIR`, `TDDY_SESSION_DIR`, and `TDDY_REMOTE_*` — parity with `ClaudeCodeBackend`.
- Managed-codebase workflow and specialized subagents extend to **cursor-cli** sessions (orchestration via `.cursor/rules/tddy-managed-workflow.mdc`). Feature: [managed-codebase-workflow.md](../managed-codebase-workflow.md), [cursor-cli-session.md](../../daemon/cursor-cli-session.md).
