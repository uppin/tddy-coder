# 2026-08-23 — The action tools are advertised where nothing implements them

**Category:** Future enhancement
**Source:** seeded-agents-on-any-placement changeset, 2026-08-23

`tddy-tools` merges `action_tool_router()` — `request_action`, `list_actions`, `invoke_action` — into
its router whenever a session-tool transport is reachable (`packages/tddy-tools/src/server.rs:313`),
which for a daemon-hosted `claude-cli` session it always is. That session's dispatch lands in
`tddy_tool_engine::execute_tool_with_env` (`packages/tddy-tool-engine/src/lib.rs:217-234`), whose
`match` has no arm for any of the three, so every call comes back
`{"error":"unknown tool: ListActions","is_error":true}`. Only `tddy-sandbox-app`'s bridge and the
`tddy-coder`-hosted `toolcall::listener` implement them.

Not merely dead surface: it misleads. In session `01a03066`, asked to invoke a specialized agent, the
main agent reached for `invoke_action`, got `unknown tool: InvokeAction`, and reported that the agent
was not registered — a wrong conclusion drawn from a tool that should not have been on offer. Either
implement the three on the daemon path (they are host round-trips: `EstablishAction`, `ListActions`,
`InvokeAction` against the session directory) or gate the merge on a transport that serves them.
