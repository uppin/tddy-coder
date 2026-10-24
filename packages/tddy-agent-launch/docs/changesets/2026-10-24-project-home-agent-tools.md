# 2026-10-24 — A main's claude-cli launch carries its tddy-tools wiring
**Type:** Fix

`project_home_launch.rs` holds what a session's role adds to its claude-cli launch. The spawn takes a
`StartRole { name, main: Option<MainLaunchPlan> }` instead of a bare role name; `LaunchSessions::start_role`
and `resumed_role` mint a main's agent credential (`SplitSessions::agent_session_token_for`) and
plan nothing for any other role. `MainLaunchPlan::launch` resolves the toolchain's `tddy-tools` and
calls `project_home_launch`, which writes the role prompt (`project_home_prompt.md`) and the MCP
config into the session dir and returns the `--allowedTools`, `--disallowedTools` (every native
filesystem/shell tool), `--append-system-prompt-file`, `--mcp-config` + `--strict-mcp-config` flags
and the `TDDY_REMOTE_*` pairs (`colocated_jail_tool_env`: agent-tool socket when one is served,
local URL otherwise). A start carries the plan out in `spawn_claude_cli_process`, after every check,
so a refused start writes nothing; a resume carries it out in `resumed_role` and appends it after
any split wiring (`RoleLaunch::after_split`). `role_allowed_tools_args` remains the role's allowlist.
See [module layout](../module-layout.md) and
[project concept § Project-home session](../../../../docs/ft/daemon/project-concept.md#project-home-session).

Code issues, re-measured with `tddy-tools analyze` (coverage + CRAP) on the final tree against
`origin/master` (`d91ea44b7`), function lengths fn line to closing brace:

- `complexity-svc-resume-claude-cli-session-resume-claude-cli-session`: 144 → 142 lines,
  complexity 13 → 13, CRAP 182 — narrowed, still open.
- `spawn_claude_cli_session_inner` (no record): 310 → 310 lines, 29 → 29 params, complexity 14, CRAP
  210 — unchanged by this change.
- `start_claude_cli_session` (no record): 119 → 119 lines.
- `claude_cli_spawn_steps.rs`: 391 → 392 production lines.

Backlog: `2026-10-24-sandboxed-main-needs-session-role` gains the note that a sandboxed main must
also carry this wiring; two entries were recorded from the diagnosis
(`2026-10-24-github-account-missing-subject-id-no-commit-identity`,
`2026-10-24-session-room-activity-broadcast-over-8k-dropped`).
