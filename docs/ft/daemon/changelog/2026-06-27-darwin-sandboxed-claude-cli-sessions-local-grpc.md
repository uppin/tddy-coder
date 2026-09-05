# 2026-06-27 — Darwin-sandboxed Claude CLI sessions (local gRPC)

- `StartSessionRequest.sandbox`: when `session_type:"claude-cli"` and `sandbox:true` on macOS, spawns `claude` inside Seatbelt via `tddy-tools sandbox-runner`; host daemon dials in-jail `SessionChannel` for PTY I/O, MCP tool exec, and LLM egress relay
- New crates `tddy-sandbox` (trait + context dir) and `tddy-sandbox-darwin` (SBPL profile + `sandbox-exec` spawn); non-macOS returns `failed_precondition`
- `ResumeSession` / `DeleteSession` stop the sandbox child and tear down the worktree; `.session.yaml` records `sandbox: true`
- Feature: [claude-cli-session.md](../claude-cli-session.md). Technical: [connection-service.md](../../../../packages/tddy-daemon/docs/connection-service.md#sandboxed-claude-code-cli-sessions), [tddy-sandbox architecture](../../../../packages/tddy-sandbox/docs/architecture.md).
