# 2026-11-09 — The roster reads the host's coding-agent registry

**Type:** Refactor

`#agent-tools` 27/32 ([#835](https://github.com/uppin/tddy-coder/pull/835)).

- `AgentRoster.coding_agents`; `coding_agent_defs_for`, `refuse_unconfinable_harness` and
  `start_coding_agent_session` take the registry; `SnapshotSources.coding_agents` feeds a coding
  agent's `transport` / `permission_policy`. An ACP conversation opens through `launch_agent` and its
  session holds the launch's hooks.

See [session-agent-roster.md](../session-agent-roster.md) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-agent-tools-acp-subagent-leftovers.md).
