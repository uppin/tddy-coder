# 2026-06-06 — Claude Code CLI session type

**Type:** Feature

`tddy-daemon` spawns and manages `claude` CLI subprocess (worktree, `--session-id`, tokio channels, bidi gRPC `StreamSessionTerminalIO`); `tddy-core` `SessionMetadata` fields; `tddy-service` proto additions; `tddy-web` session type selector, model dropdown, `GhosttyTerminalGrpc`, `ConnectedClaudeCliTerminal`. Feature [claude-cli-session.md](../../ft/daemon/claude-cli-session.md); product [daemon/changelog/](../../ft/daemon/changelog/); technical [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md), [web-terminal.md](../../ft/web/web-terminal.md). (tddy-service, tddy-core, tddy-daemon, tddy-web, docs)
