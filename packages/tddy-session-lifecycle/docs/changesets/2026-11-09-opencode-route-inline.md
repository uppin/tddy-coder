# 2026-11-09 — The daemon ACP planner has no route-shape branch for OpenCode (#agent-tools 29/32, #857)

- `acp_plan_from_spec` applies a route through its env and secret env only; the `json_config_file` branch and `opencode_route_config` are gone. A route carrying files is refused, naming its shape (`RouteShape::word`).
- `tests/launcher_names_no_agent.rs` pins that the daemon ACP launcher names no agent and no agent variable.
