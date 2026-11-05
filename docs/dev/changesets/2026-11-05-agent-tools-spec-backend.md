# 2026-11-05 — `tddy-coder --agent` builds a spec backend for any registry agent

**Type:** Refactor (with a behaviour gain: `oz` and dropped-in agents are buildable workflow backends)

`#agent-tools` 16/19 ([#822](https://github.com/uppin/tddy-coder/pull/822)), branch
`feature/agent-tools/spec-backend`, based on `feature/agent-tools/headless-driver`
([#821](https://github.com/uppin/tddy-coder/pull/821)); blocks `feature/agent-tools/agent-config-removal` (n19).
Packages: `tddy-agent-backend`, `tddy-coder`, `tddy-model-registry`, `tddy-daemon`,
`tddy-coding-agent-specs` (docs), and the `AnyBackend::Stub` test sites in `tddy-e2e`,
`tddy-livekit`, `tddy-integration-tests`, `tddy-tui-testkit`, `tddy-service`, `tddy-presenter`.

`AnyBackend`, whose seven variants named agents, is replaced by one spec-built backend:
`SpecBackend { Headless(SpecHeadlessBackend), Acp(SpecAcpBackend) }`. `tddy-coder --agent <id>`
selects the agent by its registry id and builds it from its spec, so `oz` and an agent dropped into
`<tddyhome>/tools/` run a workflow with no Rust change. No fallback: an unknown id is refused listing
the buildable ids; a spec with neither a `headless` nor an `acp` section is refused naming the agent.

- **Transport rule** (decided 2026-11-05): a spec declaring both sections runs **headless** unless
  the session or the request asks for ACP (`SpecTransport::chosen`). Documented in
  [coding-agent-specs](../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md#which-transport-a-workflow-turn-takes).
- **No hangs**: `AcpDriver::open_session` is bounded by `SESSION_OPEN_DEADLINE` (120 s,
  `initialize` included); `SpecAcpBackend` refuses an open past its deadline naming the agent. This
  narrows [2026-11-04-acp-launch-waits-have-no-deadline](../todo/2026-11-04-acp-launch-waits-have-no-deadline.md)
  to the supervisor wait.
- `tddy-agent-backend`: `backend/spec_backend.rs` (`SpecBackend`, `SpecTransport`,
  `SpecBackendRefusal`, `SpecAcpBackend`, `SharedBackend::stub()`); `SESSION_OPEN_DEADLINE`,
  `AcpDriver::open_session_within`.
- `tddy-coder`: `spec_backend_build.rs` (`spec_backend_for`, `build_spec_backend`,
  `SpecBackendRequest`, `create_workflow_backend`, `BinaryPathFlags`);
  `agent_registry_cli::reserved_agent_ids`.
- `tddy-model-registry`: `ModelRegistryStore::reserving_coding_agent_ids`; the `tddy-coder`
  dependency is gone. `tddy-daemon` (`runtime/mount_host.rs`) feeds it the host's registry ids.
- **Removed**: `AnyBackend` and its five delegating matches, `SharedBackend::from_any`,
  `AnyBackend::with_provider_route` (`routed.rs`); `tddy-coder`'s `create_backend` — together with
  `provider_route_arg::apply_provider_route` (the `AnyBackend` route step it called) and the no-op
  `on_progress` callback it passed — and `BUILTIN_BACKEND_AGENT_IDS`. Every
  `SharedBackend::from_any(AnyBackend::Stub(…))` test site uses `SharedBackend::stub()`.
- **Earlier nodes' code changed here** (a done layer is frozen, so the fix belongs to this node):
  `provider_route::by_spec` (n13, #819) is a `pub mod`; `codex_acp_route_args` is applied in the
  `CodexAcpLogin` hooks adapter's `prepare` (n11, #800) instead of `CodexAcpBackend::driver`, so the
  generic `SpecAcpBackend` routes codex-acp through its spec's adapter.
- **Deferred to the sweep (n20)**: `model_catalog::build_backend` (now `Box<dyn CodingBackend>`,
  `TODO(spec-backend)`) and the three bespoke ACP backends —
  [2026-11-05-model-listing-and-bespoke-acp-backends-remain](../todo/2026-11-05-model-listing-and-bespoke-acp-backends-remain.md).
- **Code issues**: `tddy-coder` `oversized-file-run` 2,646 → 2,502 production lines (still over,
  kept); `tddy-agent-backend` `oversized-file-backend-mod` re-measured (kept). Restructuring is the
  sweep's.
- Tests: `tddy-agent-backend/tests/spec_backend_{unit,integration}.rs`,
  `tddy-coder/tests/spec_backend_build.rs`, `tddy-coder/tests/agent_registry_cli_acceptance.rs`,
  `tddy-model-registry/tests/model_registry_reserved_names_unit.rs`.
