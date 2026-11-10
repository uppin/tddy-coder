# 2026-11-05 — The route matrix reads the loaded registry

**Type:** Refactor

`#agent-tools` 13/19 ([#819](https://github.com/uppin/tddy-coder/pull/819)).

- `HarnessRouteSupport::new(Option<Arc<CodingAgentRegistry>>)`, handed the registry the session host loaded (`mount_accounts` → `accounts_service_over`); `claude` and `claude-cli` name one spec, an unknown id or a host with no coding agents refuses.
- `tests/provider_route_listing_acceptance.rs`: `ListProviderRoutes` for a dropped-in agent offers the accounts its spec's providers serve.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-route-support.md).
