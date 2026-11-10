# 2026-11-02 — The unsandboxed claude-cli start and resume launch from the `claude` spec

**Type:** Feature

`#agent-tools` 2/12 ([#791](https://github.com/uppin/tddy-coder/pull/791)).
- New `spec_launch.rs` (`HostCodingAgents`, `HostAgent`, `launch_host_pty`, `launch_claude_cli`, `AgentToolset`, `SpecLaunchError`).
- New `host_coding_agents.rs`, moved from `tddy-daemon` so the daemon keeps only wiring.
- `LaunchSessions::coding_agents` selects the launch:
  - `spawn_claude_cli_process` → `spawn_from_spec` / `spawn_hand_built`;
  - `relaunch_claude_cli` on resume.
- On the spec launch the hooks file is written by the hooks slot (`ClaudeCliHooks`).
- A main's tool flags are `MainAgentFlags` (`RoleWiring`, `ResumedFlags`), rendered either as argv or as slot inputs.
- Code issues:
  - `complexity-svc-resume-claude-cli-session-resume-claude-cli-session`: 141 → 139 lines, narrowed and still open.
  - `complexity-claude-cli-spawn-spawn-claude-cli-session-inner`: added at 318 lines (309 at base).
  - `complexity-svc-start-session-core-start-session-core`: not touched.

See [module-layout.md](../module-layout.md) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-host.md).
