# 2026-10-24 — The launcher opens headless coding agents

**Type:** Feature

`#coagents` 7/8 ([#726](https://github.com/uppin/tddy-coder/pull/726)). `DaemonCodingAgentLauncher::headless_command_for` (`headless_plan`): the CLI the daemon's own sessions resolve, on the conversation worktree, as the daemon's own user only, with the route's environment; `claude` gets an owner-only `mcp.json` with `tddy-tools --mcp`, the `approval_prompt` prompt tool, and its approvals bound in `ApprovalRoutes`. `with_headless_wiring`, `with_approval_routes`; the host carries `approval_routes`. Tests: `coding_agent_launcher/headless_tests.rs`.

Code issues: new `oversized-file-coding-agent-launcher` (419 → 685), `oversized-file-connection-service` +3; deferred to the sweep node. See [coding-agent launcher](../coding-agent-launcher.md#a-headless-conversation).
