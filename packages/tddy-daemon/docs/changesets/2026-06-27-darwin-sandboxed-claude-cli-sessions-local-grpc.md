# 2026-06-27 — Darwin-sandboxed Claude CLI sessions (local gRPC)

**Type:** Feature

`start_sandboxed_claude_cli_session`, `SandboxSessionManager`, `dial_and_bridge` on `SessionChannel`; `ResumeSession`/`DeleteSession` stop `SandboxHandle`; `.session.yaml` `sandbox: true`; deps `tddy-sandbox`, `tddy-sandbox-darwin`; acceptance: sandbox_behavior, sandboxed_claude_cli, sandboxed_session_lifecycle. Feature [claude-cli-session.md](../../../../docs/ft/daemon/claude-cli-session.md); technical [connection-service.md](../connection-service.md#sandboxed-claude-code-cli-sessions). (tddy-daemon, tddy-sandbox, tddy-sandbox-darwin, tddy-service, tddy-tools)
