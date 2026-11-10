# 2026-11-09 — The daemon launcher prepares an ACP agent's declared hooks adapter

**Type:** Refactor

`#agent-tools` 27/32 ([#835](https://github.com/uppin/tddy-coder/pull/835)).

- `DaemonCodingAgentLauncher::launch_agent` prepares the spec's `acp.hooks_adapter` (`acp_hooks_from_spec`,
  handed no route) before the user switch (`transport_of_command`); route `-c` args are keyed on
  `route.shape: toml_args`, the inline OpenCode config on `json_config_file`, never on a harness name.
- The roster store is built over the host's coding agents (`with_coding_agents`), and `AgentRoster`
  carries them.
- Tests: `tests/acp_agents_from_spec_acceptance.rs` (codex login retry, route shape, inline OpenCode
  route, no-adapter guard).

See [coding-agent-launcher.md](../coding-agent-launcher.md) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-agent-tools-acp-subagent-leftovers.md).
