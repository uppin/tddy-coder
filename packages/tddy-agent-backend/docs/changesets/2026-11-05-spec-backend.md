# 2026-11-05 — `SpecBackend` replaces `AnyBackend`

**Type:** Refactor

`#agent-tools` 16/19 ([#822](https://github.com/uppin/tddy-coder/pull/822)).

- `backend/spec_backend.rs`: `SpecBackend { Headless(SpecHeadlessBackend), Acp(SpecAcpBackend) }`, `transport()`, `with_provider_route -> Result<Self, RouteRefusal>`; `SpecTransport::chosen` (headless when a spec declares both, unless ACP is asked); `SpecBackendRefusal::{NoTurnTransport, TransportNotDeclared}`; the generic `SpecAcpBackend` (`new`, `with_hooks`, `with_session_open_deadline`, `with_provider_route`; system prompt merged, hooks' open scope bracketing each open); `SharedBackend::stub()`.
- `AcpDriver::open_session` is bounded by `SESSION_OPEN_DEADLINE` (120 s); `open_session_within(open, deadline)` refuses naming the program and the deadline.
- Deleted: `AnyBackend` and its five delegating matches, `SharedBackend::from_any`, `AnyBackend::with_provider_route` (`routed.rs`).
- Changed in earlier nodes' code: `provider_route::by_spec` is a `pub mod` (n13's module; `SpecAcpBackend` reaches `agent_routable` through it); `codex_acp_route_args` is applied by the `CodexAcpLogin` hooks adapter's `prepare` (n11's adapter) instead of `CodexAcpBackend::driver`, so `SpecAcpBackend` gets codex-acp's route overrides through the spec's adapter.
- `model_catalog::build_backend` returns `Box<dyn CodingBackend>`, marked `TODO(spec-backend)` for the sweep ([backlog](../../../../docs/dev/todo/2026-11-05-model-listing-and-bespoke-acp-backends-remain.md)).
- Tests: `tests/spec_backend_unit.rs`, `tests/spec_backend_integration.rs`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-spec-backend.md).
