# 2026-11-09 — The relayed-agent refusal names exec_roots and the env rule

**Type:** Fix

`#agent-tools` 32/32 ([#865](https://github.com/uppin/tddy-coder/pull/865)).

- `coding_agent_relay.rs` turns the supervisor's opaque refusal of `SpawnRelayed` into an error naming
  three things: the harness under `exec_roots`, the user in `allowed_session_users`, and a launch
  environment with no loader variable. It no longer names `allowed_relay_tool_paths` or
  `allowed_env_keys`. Tests: `the_supervisors_opaque_refusal_names_the_exec_roots` and
  `the_supervisors_opaque_refusal_names_the_env_rule`.
- `coding_agent_launcher.rs` passes no cgroup scope to `start_relayed`. Backlog:
  `docs/dev/todo/2026-11-09-daemon-creates-no-cgroup-scope-for-supervised-spawns.md`.
- Docs: [coding-agent-launcher.md](../coding-agent-launcher.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-supervisor-agent-agnostic.md).
