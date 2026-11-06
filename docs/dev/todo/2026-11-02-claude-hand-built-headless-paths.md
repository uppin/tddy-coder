# 2026-11-02 — Claude's hand-built headless paths outlive the spec path

**Category:** Deferred from `#agent-tools` 6/12 (#795)
**Source:** `/green` and `/validate-changes` on `feature/agent-tools/claude-headless`; `TODO(claude-headless)` at
`packages/tddy-agent-backend/src/backend/claude.rs` (`ClaudeCodeBackend::headless`),
`packages/tddy-agent-backend/src/backend/headless_driver.rs` (`HeadlessCommand::renderer`) and
`packages/tddy-session-lifecycle/src/coding_agent_launcher.rs` (`coding_agents`)

## What is there

Every production path now renders Claude from `claude.yaml`'s `coding_agent.headless` section
(`tddy-coder --agent claude`, a daemon's headless coding subagent). The hand-built argv is still
compiled and reachable from callers that construct a backend without a renderer:

| Code | Kept for |
|---|---|
| `build_claude_args` (`pub`) | `packages/tddy-integration-tests/tests/backend_args.rs` |
| `create_mcp_config_temp_file` | renderer-less `ClaudeCodeBackend` (`with_path` in integration tests) |
| `headless_driver/argv.rs::claude()` | `tests/headless_driver_acceptance.rs` and the argv unit tests (`HeadlessDriver::new(Claude, …)` without a renderer) |
| the launcher's hand-built Claude `mcp_servers_wrapped` | `DaemonCodingAgentLauncher` built without coding agents (`with_identity`, launcher tests) |

## Why it was deferred

#795's Responsibility planned deleting `argv.rs::claude()`, but each needs its tests moved onto a
renderer first, and deleting public functions needs developer approval. The goldens in
`tddy-coding-agent-specs/tests/headless_render_golden.rs` pin the two paths byte-identical, so they
cannot drift silently meanwhile.

## What would close it

Move those callers onto `SpecHeadlessRenderer` (or a fixture renderer, as
`tests/headless_renderer_port.rs` does), then delete the four. Natural owner: #798 headless-others,
which moves Cursor, Codex and Oz the same way, or the `#agent-tools` sweep node.
