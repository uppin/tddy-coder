# 2026-11-05 — A provider route is chosen by the spec's route support

**Type:** Refactor (breaking for override files)

`#agent-tools` 13/19 ([#819](https://github.com/uppin/tddy-coder/pull/819)). Packages:
`tddy-agent-backend`, `tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-session-lifecycle`,
`tddy-daemon` (plus the key-variable rename in `tddy-integration-tests`).

- `tddy-agent-backend`: `ProviderKind`, `RouteShape`, `RouteSupport` move here from
  `tddy-coding-agent-specs` (which re-exports them), beside `RoutedAgent { id, product, support,
  key_env }` and `bodies_accept(shape)`. The route body is chosen by the spec's (shape, provider
  served); `RouteRefusal` is agent-keyed and names the product. `provider_route::Harness` and every
  closed agent table it carried are deleted.
- `tddy-coding-agent-specs`: `coding_agent.product` on every built-in spec; `routed_agent(&spec)`;
  shape words renamed after the body format (`args` → `toml_args`, `config_file` →
  `json_config_file`; an override still writing an old word is refused at load naming the new one);
  `spec_for_session_type` deleted.
- `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-daemon`: every string-keyed route caller
  resolves the agent through the host's `CodingAgentRegistry`; an unknown id is refused, never routed
  as Claude Code. The daemon's `HarnessRouteSupport` reads the registry the session host loaded, once.
- **The agent process sees no `TDDY_*` route variable**: Codex's key is in `CODEX_PROVIDER_KEY`
  (was `TDDY_ROUTE_API_KEY`) and OpenCode's in `OPENCODE_PROVIDER_KEY` (was
  `TDDY_OPENCODE_PROVIDER_KEY`), each its spec's `route.key_env`. Pinned over the built-in registry by
  `no_built_in_route_hands_the_agent_process_a_tddy_variable`.
- A coding agent dropped into `<tddyhome>/tools/` with a `route` section is routable through the same
  bodies, and `ListProviderRoutes` offers it the accounts its providers serve.

Backlog resolved (deleted at this wrap):

- `2026-11-04-provider-route-harness-enum-remains` — "`provider_route::Harness` still enumerates the
  agents a route body is chosen for". Final measurement: 0 references to `provider_route::Harness`,
  `Harness::IDS` or `from_agent_or_session_type` in `packages/*/src` and `packages/*/tests`.
- `2026-11-04-spec-for-session-type-is-unused` — "`spec_for_session_type` is replaced by the registry
  but still has one caller". Final measurement: 0 references; `harness_profile_start::profile_spec_id`
  reads `CodingAgentRegistry::for_session_type`.

Code issues:

- Closed and deleted: `tddy-agent-backend/docs/code-issues/oversized-file-provider-route.md` —
  `src/provider_route.rs` 502 → 186 production lines (budget 500, counted to the first
  `#[cfg(test)]`); `provider_route/by_spec.rs` is 320 and `provider_route/support.rs` 126.
- Still open, deferred to the stack's sweep node (developer, 2026-11-04): `oversized-file-spec`
  (711 → 736), `oversized-file-coding-agent-launcher` (1291 → 1294), and the `tddy-agent-launch`
  complexity records this PR touched at a call site, each re-measured in its own file.
