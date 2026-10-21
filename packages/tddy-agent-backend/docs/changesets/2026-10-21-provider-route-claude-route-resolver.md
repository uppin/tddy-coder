# 2026-10-21 — Provider route resolver and capability matrix

**Type:** Feature

`#provider-route` 2/9 ([#663](https://github.com/uppin/tddy-coder/pull/663)). New `provider_route` module (re-exported as `tddy_core::provider_route`): `Harness` for the ten
harness ids, `RouteTarget` / `RouteEndpoints` / `RouteSecret` (redacted `Debug`), `HarnessRoute`
(`env`, `secret_env`, `files`, `args`), `RouteRefusal`, the matrix `route_supported`, and
`resolve_route` with the Claude Code body (`ANTHROPIC_BASE_URL`, secret `ANTHROPIC_AUTH_TOKEN`, the
five model pins, `--model`); Codex, OpenCode and Muse Code answer `NotYetRoutable`, Cursor
`HarnessCannotBeRouted`. `RouteChoice` / `RouteSources` / `choose_route` carry the session > agent
definition > user default precedence. See [architecture](../architecture.md#provider-routes-provider_route).
