# 2026-11-09 — The crate names no agent; an ACP session holds its launch's hooks

**Type:** Refactor

`#agent-tools` 27/32 ([#835](https://github.com/uppin/tddy-coder/pull/835)).

- Transport, permission policy, catalog and confinement are read from the host's
  `CodingAgentRegistry`: `transport_of_spec`, `permission_policy_of_spec`,
  `transport_and_policy_wire_of_spec`, `coding_agent_defs_from`, `refuse_unless_registry_confinable`,
  `confinable_acp_agents`. The agent tables and name-keyed withdrawals are deleted; `tests/no_agent_names.rs`
  pins that no production line names an agent.
- `CodingAgentLauncher::launch_agent` → `LaunchedAgent { transport, hooks }`, `no_launch_hooks()`;
  `session_meta_for` is required. `CodingAgentSession::start` / `start_over` take the launch's
  `PreparedAcpHooks`, bracket the open with them and hold them for the session's lifetime.
- `session.rs` split into `session/{cancel, subagent_session, message_shape, turn_rules}.rs` (756 → 445).
- Tests: `tests/registry_driven_agents.rs`, `tests/session_hooks_lifetime.rs`, `tests/no_agent_names.rs`.

See [architecture.md](../architecture.md) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-agent-tools-acp-subagent-leftovers.md).
