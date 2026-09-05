# 2026-06-27 — **Darwin-sandboxed Claude CLI

**Type:** Feature

sandbox-runner + MCP allowlist** — `sandbox-runner` subcommand: in-jail gRPC `SessionChannel`, PTY relay, tool IPC → `ExecuteToolRequest`, in-jail HTTP shim → `EgressRequest`; `sandbox_claude_spawn.rs` writes MCP config and `--allowedTools`; `tddy-demo-tui` dev-dep for acceptance tests. Feature [claude-cli-session.md](../../../../docs/ft/daemon/claude-cli-session.md). (tddy-tools, tddy-sandbox)
