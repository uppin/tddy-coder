# 2026-07-06 — Cursor CLI sandbox parity with Claude CLI

- **`session_type = "cursor-cli"` + `sandbox = true`** succeeds on macOS (Seatbelt) and Linux (cgroups+namespaces) via `start_sandboxed_cursor_cli_session` and `tddy-sandbox-recipes::cursor_cli`; managed codebase, specialized subagents, and `TDDY_SOCKET` workflow wiring mirror claude-cli.
- In-jail `agent` spawns via direct `node index.js`; MCP config via `$HOME/.cursor/mcp.json` (no auto-injected `--approve-mcps` / `--force` / `--trust`).
- **`WaitingForInput`** remains unmapped (documented gap); sandboxed cursor-cli resume relaunch and jail Keychain auth are follow-ups. Feature: [cursor-cli-session.md](../cursor-cli-session.md). PR [#287](https://github.com/uppin/tddy-coder/pull/287).
