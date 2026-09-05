# 2026-06-27 — Darwin sandbox protos

**Type:** Feature

`sandbox.proto`: `SandboxService` with bidi `SessionChannel` (PTY, `ExecuteToolRequest`/`Response`, `EgressRequest`/`Response`); `connection.proto`: `StartSessionRequest.sandbox` (bool); TypeScript + tonic codegen. Feature [claude-cli-session.md](../../../../docs/ft/daemon/claude-cli-session.md). (tddy-service, tddy-daemon, tddy-tools)
