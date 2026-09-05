# 2026-06-27 — Darwin-sandboxed Claude CLI sessions

**Type:** Feature

new crate: `SandboxSpec`, `SandboxHandle`, `SandboxError::Unsupported`, `SandboxContextDir` (read-only context + `REMOTE_APPENDIX`), spawn facade; acceptance: `unsupported_on_non_darwin`. Feature [claude-cli-session.md](../../../../docs/ft/daemon/claude-cli-session.md); architecture [architecture.md](../architecture.md). (tddy-sandbox, tddy-sandbox-darwin, tddy-daemon, tddy-tools, tddy-service)
