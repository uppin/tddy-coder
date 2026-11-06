# 2026-11-05 — Route types re-exported; product and routed_agent

**Type:** Refactor (breaking for override files)

`#agent-tools` 13/19 ([#819](https://github.com/uppin/tddy-coder/pull/819)).

- `route.rs` re-exports `tddy_agent_backend::provider_route::{ProviderKind, RouteShape, RouteSupport, RoutedAgent}` under the paths 12/19 published.
- `coding_agent.product` (`CodingAgentSpec::product()`, the id when absent), declared by every built-in; `routed_agent(&CodingAgentSpec)`.
- Built-in specs: `route.shape` words `toml_args` (codex, codex-acp) and `json_config_file` (opencode); `route.key_env` `CODEX_PROVIDER_KEY` (codex, codex-acp) and `OPENCODE_PROVIDER_KEY` (opencode).
- Removed: `spec_for_session_type` and its two unit tests; `CodingAgentRegistry::for_session_type` answers it.
- `tests/route_body_drift_acceptance.rs`: each built-in's providers against `bodies_accept`, the removed shape words, every built-in's product, a routed Codex's key in `CODEX_PROVIDER_KEY`, and no built-in route handing the agent a `TDDY_*` variable.
- Code issue `oversized-file-spec`: 711 → 736 production lines, still open, deferred to the stack's sweep.

See [coding-agent-specs.md](../coding-agent-specs.md) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-route-support.md).
