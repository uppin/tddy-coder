# 2026-10-22 — CLI harness registry and the OpenCode route

**Type:** Feature

`#provider-route` 3/9 ([#664](https://github.com/uppin/tddy-coder/pull/664)). New `cli_harness` module (re-exported as `tddy_core::cli_harness`): `CliHarness`, `CliLaunch`,
`CliModelSource`, `cli_harnesses()` / `cli_harness(session_type)` with the claude-cli, cursor-cli and
opencode-cli descriptors, and `models_from_listing`. `resolve_route` routes `OpenCode` /
`OpenCodeCli` (`opencode.json` file, `TDDY_OPENCODE_PROVIDER_KEY`, `--model tddy-route/<model>`);
`opencode_config_for(target) -> Option<RouteFile>`, `OPENCODE_ROUTE_PROVIDER_ID`,
`OPENCODE_ROUTE_KEY_ENV`. `resolve_agent_models` answers a CLI harness from its descriptor
(`BackendCliPaths.opencode_cli_path`); `context_globs_for_agent("opencode")`. Code issue
`panicking-api-provider-route-opencode-config-for` closed (1 → 0 panics) and deleted;
`oversized-file-backend-mod` 877 → 879, deferred as stack-shared. See
[architecture](../architecture.md#cli-harnesses-cli_harness).
