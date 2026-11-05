# 2026-11-05 — Routes resolved from the host's coding agents

**Type:** Refactor

`#agent-tools` 13/19 ([#819](https://github.com/uppin/tddy-coder/pull/819)).

- `session_provider_route`: `resolve_session_route` / `route_on_account` take `&RoutedAgent`; `start_route`, `resume_route`, `resume_route_env` and `sandboxed_start_route` take a lazy `RouteAgent` (`Built`, `OnHost`, `RouteAgent::claude_on(agents)`), so a session that chose no account never needs a spec. `routed_agent_in` / `routed_agent_on_host` resolve an agent id or CLI session type through the registry; an unknown id is refused, never resolved to Claude Code. `no_provider_route_serves_the_agent` removed.
- `CliSessionType.route` is the spec's `RoutedAgent`; `cli_route_launch::routed_launch_model` blanks the argv's model when the route's own arguments name one.
- `tool_session_route_json` and `harness_profile_start::{profile_start, profile_spec_id}` take the host's coding agents.

See [module-layout.md](../module-layout.md) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-route-support.md).
