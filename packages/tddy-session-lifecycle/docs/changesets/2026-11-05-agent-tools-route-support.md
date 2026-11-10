# 2026-11-05 — Subagent routes read from the registry

**Type:** Refactor

`#agent-tools` 13/19 ([#819](https://github.com/uppin/tddy-coder/pull/819)).

- `coding_agent_launcher`: a subagent definition's route is built from its agent's spec in the host's registry (`routed_agent_on_host`), not from the closed `Harness` table.
- `tests/yaml_only_agent_acceptance.rs`: a dropped-in agent with an `env`/`anthropic` route starts on a provider account with the account's endpoint in its environment; one without `route` is refused naming its product and spawns nothing; a harness profile naming it starts its session type.
- Code issue `oversized-file-coding-agent-launcher`: 1291 → 1294 production lines, still open, deferred to the stack's sweep.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-route-support.md).
