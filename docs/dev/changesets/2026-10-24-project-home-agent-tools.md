# 2026-10-24 — Project-home main wired to tddy-tools, native tools withdrawn, role prompt
**Type:** Fix

A project's main was launched with its tool set pre-approved but no `tddy-tools` MCP server, no
`TDDY_REMOTE_*` transport, its native shell and file tools intact and no role prompt, so it could not
list or start the project's sessions (diagnosed on Tddy Desktop). The launch now carries all four,
at start and resume, with an agent credential minted from the verified caller.

- `tddy-agent-launch`: `project_home_launch.rs` (`StartRole`, `MainLaunchPlan`, `RoleLaunch`) — see
  [its entry](../../../packages/tddy-agent-launch/docs/changesets/2026-10-24-project-home-agent-tools.md).
- `tddy-session-split`: shared `tddy_tools_only_mcp_args`, public agent mint — see
  [its entry](../../../packages/tddy-session-split/docs/changesets/2026-10-24-project-home-agent-tools.md).
- `tddy-session-lifecycle` (tests): the project-home fixture signs session tokens; acceptance cases
  pin the main's MCP config, transport, credential, withdrawn tools and role prompt, at start and resume.

Feature: [project concept § Project-home session](../../ft/daemon/project-concept.md#project-home-session).
