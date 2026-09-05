# 2026-06-06 — **Claude Code CLI session type

**Type:** Feature

web UI** — **`ConnectionScreen`**: session type selector + model dropdown; **`ConnectedClaudeCliTerminal`** bidi gRPC stream component; **`GhosttyTerminalGrpc`** (`GrpcStream` interface, output buffer before ready, OSC resize, optional chrome bar); **`constants/claudeCliModels.ts`** (`CLAUDE_CLI_MODELS`, `isClaudeCliSession`); `multiSessionState` extended with optional `claudeCli` discriminant. Tests: `claudeCliModels.test.ts`, `GhosttyTerminalGrpc.cy.tsx`. Feature [web-terminal.md](../../../../../docs/ft/web/web-terminal.md); product [daemon/changelog/](../../../../../docs/ft/daemon/changelog/). (tddy-web)
