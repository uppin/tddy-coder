# 2026-11-05 — A provider route is chosen by the spec's route support

**Type:** Refactor (breaking for override files)

`#agent-tools` 13/19 ([#819](https://github.com/uppin/tddy-coder/pull/819)).

- `provider_route/support.rs`: `ProviderKind`, `RouteShape::{Env, TomlArgs, JsonConfigFile}` (a removed `args` / `config_file` word refused naming its replacement), `RouteSupport`, `RoutedAgent { id, product, support, key_env }`, `bodies_accept(shape)`.
- `provider_route/by_spec.rs` (re-exported at `provider_route`): agent-keyed `RouteRefusal` (`AgentCannotBeRouted`, `ProtocolNotServed`, `PresetNotAccepted`, `NotYetRoutable`), `agent_routable`, `route_supported`, `resolve_route` choosing the body by (shape, provider served): `anthropic_env_route`, `key_env_route`, `toml_args_route`, `json_config_file_route`. Every body but the Anthropic-env one hands the key in the spec's `route.key_env` and refuses an agent without one.
- Removed: `provider_route::Harness` (`IDS`, `ALL`, `id`, `product`, `from_agent_or_session_type`), `harness_routable`, `route_body`, `CliHarness.route_harness`, `OPENCODE_ROUTE_KEY_ENV`, `MUSE_ROUTE_KEY_ENV`, `CODEX_ROUTE_KEY_ENV`. Codex's key moves from `TDDY_ROUTE_API_KEY` to `CODEX_PROVIDER_KEY`, OpenCode's from `TDDY_OPENCODE_PROVIDER_KEY` to `OPENCODE_PROVIDER_KEY`.
- Code issue closed: `oversized-file-provider-route` — `provider_route.rs` 502 → 186 production lines (budget 500); the bodies left for `by_spec.rs` and `support.rs`.

See [architecture.md](../architecture.md#provider-routes-provider_route) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-route-support.md).
