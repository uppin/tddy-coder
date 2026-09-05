# 2026-06-27 — Darwin-sandboxed Claude CLI sessions (local gRPC)

**Type:** Feature

new `tddy-sandbox` + `tddy-sandbox-darwin` crates; `SessionChannel` proto; `tddy-tools sandbox-runner` in-jail server; daemon `start_sandboxed_claude_cli_session` + lifecycle; MCP allowlist via `sandbox_claude_spawn.rs`; `(deny network*)` Seatbelt with host-relayed LLM egress. Feature [claude-cli-session.md](../ft/daemon/claude-cli-session.md), [remote-codebase-mode.md](../ft/daemon/remote-codebase-mode.md); technical [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md#sandboxed-claude-code-cli-sessions), [tddy-sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md). PR [#241](https://github.com/uppin/tddy-coder/pull/241). (tddy-sandbox, tddy-sandbox-darwin, tddy-service, tddy-daemon, tddy-tools, tddy-core, tddy-workflow-recipes)
