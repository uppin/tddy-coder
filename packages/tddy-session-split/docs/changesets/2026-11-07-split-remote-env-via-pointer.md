# 2026-11-07 — The split agent reaches its remote through its tool server and a pointer

**Type:** Feature

`#agent-tools` 25/28 ([#833](https://github.com/uppin/tddy-coder/pull/833)). `split_agent_spec`,
`split_claude_extra_args`, `build_split_context_dir` and `prepare_split_agent_wiring` take the spec read
from `SplitSessions.coding_agents`; `tool_server_env` (transport and roster seed) goes on the `tddy-tools`
registration, never the process. `split_session_pointer` and `SplitAgentWiring.session_pointer`, the
`SplitSessionPointers` port (`place`, `remove`). `write_split_agent_metadata` moved to
`svc_split_agent_metadata.rs`. `tddy_tools_only_mcp_args` is deleted.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-split-remote-env-via-pointer.md) and
[module-layout.md](../module-layout.md).
