# 2026-07-06 — **`cursor_cli` recipe

**Type:** Feature

Seatbelt/cgroups sandbox for Cursor Agent CLI** — `build_cursor_sandbox_argv` resolves the `agent` bundle to direct `node index.js` (bypasses bash wrapper `realpath` failure under Seatbelt); `cursor_agent_prerequisite_reads` + `path_traversal_reads` for Node module resolution; writes jail `$HOME/.cursor/mcp.json` (no auto-injected `--approve-mcps`/`--force`/`--trust`). Feature [cursor-cli-session.md](../../../../docs/ft/daemon/cursor-cli-session.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#287](https://github.com/uppin/tddy-coder/pull/287). (tddy-sandbox-recipes, tddy-sandbox-runner, tddy-sandbox-app, tddy-daemon, tddy-sandbox)
