# 2026-03-19 — ACP Backend Implementation

**Type:** Feature

ClaudeAcpBackend speaks ACP to spawned @zed-industries/claude-agent-acp subprocess via agent-client-protocol SDK. Dedicated thread with LocalSet (SDK !Send). TddyAcpClient accumulates session notifications, auto-approves permissions. Session mapping (Fresh/Resume), progress events (TaskProgress, ToolUse, TaskStarted). New tddy-acp-stub crate for testing. tddy-coder: --agent claude-acp. (tddy-core, tddy-acp-stub, tddy-coder)
