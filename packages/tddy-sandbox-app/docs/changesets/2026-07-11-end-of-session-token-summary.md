# 2026-07-11 — End-of-session token summary

**Type:** Feature

after the terminal bridge returns, `print_token_summary` merges the main Claude agent's transcript usage, its nested Task subagents, and the tddy subagent `accounting.json`, printing a per-agent breakdown + TOTAL row to stderr (Cursor sessions skip the main-agent row). Feature [session-token-accounting.md](../../../../docs/ft/coder/session-token-accounting.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#289](https://github.com/uppin/tddy-coder/pull/289). (tddy-sandbox-app)
