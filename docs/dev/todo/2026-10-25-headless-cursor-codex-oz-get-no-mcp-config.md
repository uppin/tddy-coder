# 2026-10-25 — Headless `cursor`, `codex` and `oz` subagents get no MCP config

**Category:** Deferred from `#coagents` 7/8 (#726, `headless-driver`) — developer-decided known limit
**Source:** `packages/tddy-session-lifecycle/src/coding_agent_launcher.rs` (`DaemonCodingAgentLauncher::headless_plan`, the `HeadlessHarness::Cursor | Codex | Oz => (None, None)` arm); `packages/tddy-agent-backend/src/backend/headless_driver/argv.rs`

A headless `claude` conversation is handed `tddy-tools --mcp` through an owner-only `mcp.json` and
its permission prompts are routed to `mcp__tddy-tools__approval_prompt`, which asks the daemon's
`ExecToolService.RequestApproval`. The other three headless harnesses get nothing:

- `cursor` — `HeadlessCommand.mcp_config` is never written to `<wd>/.cursor/mcp.json`, so the CLI has
  no tddy-tools tools; it runs under its fixed read-only policy.
- `codex`, `oz` — the argv builders ignore `mcp_config` (`oz` has `--mcp <json|path>`, observed; codex
  takes MCP servers through `-c mcp_servers.*`).

So none of the three has an approval route or the session's exec tools, and since none can withdraw
its native tools they are refused in managed sessions anyway.

## What would close it

Write each CLI's MCP config in its own shape (cursor's `.cursor/mcp.json` in the conversation
worktree, `oz --mcp <path>`, codex `-c` overrides) from the same `tool_server_spec`, and prove per CLI
whether a tool-withdrawal mode exists so `confinement_for` can move from `CannotWithdraw`.
