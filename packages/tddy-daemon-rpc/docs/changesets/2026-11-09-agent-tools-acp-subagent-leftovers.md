# 2026-11-09 — Coding-agent catalog rows read the host's registry

**Type:** Refactor

`#agent-tools` 27/32 ([#835](https://github.com/uppin/tddy-coder/pull/835)).

- `coding_agent_rows` / `coding_agent_info` take the host's `CodingAgentRegistry`; a row's
  `transport` / `permission_policy` come from its spec (`tests/coding_subagent_acceptance.rs`).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-agent-tools-acp-subagent-leftovers.md).
