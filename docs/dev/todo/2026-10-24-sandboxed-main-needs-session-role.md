# 2026-10-24 — A sandboxed main carries its role but not the main's tool wiring

**Category:** Future enhancement
**Source:** 2026-10-24-project-home-from-new-session-form
**Narrowed:** 2026-11-02 by #794 (`#agent-tools` 5/12)

## Resolved by #794

The sandboxed claude-cli start now carries `session_role`
(`start_sandboxed_claude_cli_from_request` → `start_sandboxed_claude_cli_session` →
`LaunchSessions::launch_jailed`, which calls `record_role_before_spawn`), and
`write_jail_session_metadata` keeps it. `refuse_what_main_cannot_honour`
(`packages/tddy-agent-launch/src/project_home_start_spec.rs`) no longer refuses `sandbox`. The
acceptance case
`ensure_project_home_session_acceptance::a_project_home_main_set_up_in_a_sandbox_is_listed_with_session_role_project_home`
pins it (macOS). `sandboxed_codebase` is still refused.

## What is left

- **The main's tool wiring in the jail** (2026-10-24-project-home-agent-tools): an unsandboxed main is
  launched through `packages/tddy-agent-launch/src/project_home_launch.rs` (`StartRole`,
  `MainLaunchPlan`: `tddy-tools --mcp` as its only MCP server, `TDDY_REMOTE_*` under a minted agent
  credential, native tools withdrawn, the role prompt). A jailed main gets none of that: its
  `--allowedTools` / `--disallowedTools` are the ordinary session's (`claude_jail.rs`,
  `build_claude_allowlist`), and no role prompt is appended. The role is in its record, so the
  daemon's read-only refusal in `ExecuteTool` applies; what is missing is the agent-side narrowing.
- **What would close it:** have `launch_jailed_claude` take the role's plan (allowed tools, prompt
  file, server env) when `session_role` is `project-home`, with `tddy-tools` reached from inside the
  jail — check whether the jail can reach the agent-tool socket / local URL before reusing
  `colocated_jail_tool_env` as-is — and add an acceptance case asserting the jailed main's argv
  carries the role's `--allowedTools`.
- **Resume:** a relaunch passes no role (`role: ""`); the start's record keeps it, but a resume that
  must re-apply the role's `--allowedTools` needs the same plan.
