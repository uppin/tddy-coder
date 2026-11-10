# 2026-11-05 — `tddy-coder --agent` builds a spec backend for any registry agent

**Type:** Refactor

`#agent-tools` 16/19 ([#822](https://github.com/uppin/tddy-coder/pull/822)).

- `spec_backend_build.rs`: `spec_backend_for(tools_dir, id, host, transport)`, `build_spec_backend(tools_dir, id, host, SpecBackendRequest { transport, provider_route })` (`stub` → `StubBackend`, a route refused; an unknown id refused listing `backend_agent_ids`; a spec with neither `headless` nor `acp` refused naming it), and `create_workflow_backend` (specialized agent def first, then the host with the `--*-path` flags, `BinaryPathFlags`). `oz` and a dropped-in agent build.
- `agent_registry_cli::reserved_agent_ids(registry)`: every registry id, then `stub`. `acp_from_spec::acp_launch` is `pub(crate)`.
- Deleted with `create_backend`: `BUILTIN_BACKEND_AGENT_IDS`, `provider_route_arg::apply_provider_route` (its `AnyBackend` route step, now `build_spec_backend`'s) and the no-op `on_progress` callback `create_backend` passed. `create_backend_specialized_agent_tests` became `create_workflow_backend_specialized_agent_tests`.
- `run.rs` 2,646 → 2,502 production lines.
- Tests: `tests/spec_backend_build.rs`, `tests/agent_registry_cli_acceptance.rs` (a dropped-in agent runs a turn, `oz` builds, an unknown agent is refused).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-spec-backend.md).
