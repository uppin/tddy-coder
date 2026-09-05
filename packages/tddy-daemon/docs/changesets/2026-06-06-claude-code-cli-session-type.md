# 2026-06-06 — Claude Code CLI session type

**Type:** Feature

**`claude_cli_session`**: **`ClaudeCliSessionManager`** (tokio-channel subprocess registry; `start()` spawns `claude --model <m> --session-id <id>`; broadcast stdout, mpsc stdin; exit monitor removes from registry); **`connection_service`**: `start_session` claude-cli branch, `connect_session` early return, `stream_session_terminal_io` bidi handler, `delete_session` worktree cleanup; **`session_list_enrichment`** populates `agent = "claude-cli"` and `model`; **`config`** optional `claude_cli.binary_path`. Tests: `claude_cli_session_acceptance`. Feature [claude-cli-session.md](../../../../docs/ft/daemon/claude-cli-session.md); technical [connection-service.md](../connection-service.md); product [daemon/changelog/](../../../../docs/ft/daemon/changelog/). (tddy-service, tddy-core, tddy-daemon, tddy-web, docs)
