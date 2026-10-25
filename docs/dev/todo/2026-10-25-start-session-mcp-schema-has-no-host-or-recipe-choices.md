# 2026-10-25 — The agent sees StartSession's host and recipe as free strings

**Category:** Deferred from `#start-parity` 2/9 (#751) — follow-up
**Source:** `tddy_tool_engine::start_session_args::start_session_schema_without_choices`

`ListExecTools` answers a project-home session with `StartSession`'s schema listing every eligible
host and supported recipe as enums, but `tddy-tools --mcp` registers its MCP tool definitions from
`project_home_tool_defs()` before it asks the daemon, so the agent is shown `daemon_instance_id` and
`recipe` as free strings. The daemon still refuses a host it cannot route to and a recipe it does not
support, so nothing wrong is started — the agent just learns the choices by being refused.

**Why deferred:** registering from the daemon's answer changes how `tddy-tools` builds its MCP server,
outside #751's packages.

**What would close it:** have `tddy-tools --mcp` register the project-home tools from the daemon's
`ListExecTools` answer (it already relays the call), and assert the enums in an MCP-level test.
