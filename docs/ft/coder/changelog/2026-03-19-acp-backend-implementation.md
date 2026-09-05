# 2026-03-19 — ACP Backend Implementation

- **ClaudeAcpBackend**: New `CodingBackend` that speaks ACP (Agent Client Protocol) to `@zed-industries/claude-agent-acp` subprocess via agent-client-protocol Rust SDK. Dedicated thread with LocalSet for !Send SDK. Session mapping (Fresh/Resume), progress events (TaskProgress, ToolUse, TaskStarted).
- **tddy-acp-stub**: New crate implementing `acp::Agent` for testing. Scenario-based responses (chunks, tool_calls, permission_requests).
- **CLI**: `--agent claude-acp` selects ACP backend. `verify_tddy_tools_available` skips check for claude-acp.
- **Packages**: tddy-core (backend/acp.rs), tddy-acp-stub (new), tddy-coder (run.rs).
