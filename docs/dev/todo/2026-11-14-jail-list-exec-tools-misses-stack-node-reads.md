# 2026-11-14 — The jail `ListExecTools` bridge does not offer a stack node's two read tools

**Category:** Deferred from `#orchestrators` 7/11 (stack-plan-tools)
**Source:** `packages/tddy-session-lifecycle/src/connection_service/svc_list_exec_tools_wiring.rs` (`TODO(stack-plan-tools)`)

The daemon's exec-tool catalogue offers a session bound to a node of its orchestrator's stack
(`StackCallerRole::StackNode` in `tddy-tool-engine::stack_tools`) only `pr_stack_status` and
`pr_read` for that stack. The daemon-side `ListExecTools` (`tddy-daemon-rpc`) appends those two
via `stack_caller_of`, which reads the session's changeset and its orchestrator's stack record.

The jail bridge (`list_exec_tools_from_jail`) answers from
`session_offered_catalog_for(session_tool_role(...))` alone. It cannot classify a node binding:
that needs the project stack store and the orchestrator's changeset, which live behind
`tddy-daemon-rpc`, and `tddy-session-lifecycle` cannot depend on it (the dependency runs the
other way). A node session running inside a jail is therefore not advertised its two read tools
through this path, even though the daemon would serve them.

**Why deferred:** closing it needs the node-binding facts carried into this crate (for example a
shared classification helper in `tddy-tool-engine` fed by store reads this crate can do, or the
binding recorded on the session record at spawn). That is a design choice beyond n7's boundary,
which keeps classification in tool-engine + daemon and leaves `tddy-agent-launch` untouched.

## What would close it

Make `list_exec_tools_from_jail` append `stack_tools_offered_to` for a node-bound bound session,
with a test in `svc_list_exec_tools_wiring.rs` mirroring
`a_stack_node_session_is_offered_only_status_and_read_for_its_own_stack` on the daemon side.
