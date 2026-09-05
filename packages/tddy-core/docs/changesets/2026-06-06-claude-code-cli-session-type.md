# 2026-06-06 — **Claude Code CLI session type

**Type:** Feature

`SessionMetadata` fields** — **`session_type: Option<String>`** and **`model: Option<String>`** on **`SessionMetadata`**; **`InitialToolSessionMetadataOpts`** extended; **`write_initial_claude_cli_session_metadata()`** convenience wrapper; backward-compatible YAML serde. Tests: `claude_cli_metadata_round_trip`. Feature [claude-cli-session.md](../../../../docs/ft/daemon/claude-cli-session.md). (tddy-core)
