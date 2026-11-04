# Changeset: `SpecBackend` replaces `AnyBackend`; `tddy-coder --agent` builds any registry agent

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Refactor (with a behaviour gain: `oz` and dropped-in agents are buildable workflow backends)
**Complexity**: 4 — more than one crate (`tddy-agent-backend`, `tddy-coder`, `tddy-model-registry`, plus `AnyBackend::Stub` test sites in `tddy-e2e`, `tddy-livekit`, `tddy-integration-tests`, `tddy-tui-testkit`, `tddy-service`, `tddy-presenter`); a public-type removal; a new deadline on the ACP session open. Not 5: the backend halves already exist (#821's driver, `AcpDriver`), no proto, no data migration
**Stack:** `#agent-tools 16/19` — branch `feature/agent-tools/spec-backend`, base `feature/agent-tools/headless-driver` (#821)
**PRD:** none of its own. Delivers the "`tddy-coder --agent` accepts any registry id" criterion of the registry PRD on `feature/agent-tools/agent-registry` (`docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`)

## Initial Discovery

[2026-11-04-agent-tools-spec-backend-initial-discovery.md](./2026-11-04-agent-tools-spec-backend-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-acp-launch-waits-have-no-deadline.md](../todo/2026-11-04-acp-launch-waits-have-no-deadline.md) | ◐ **PARTLY RESOLVED HERE.** `AcpDriver::open_session` gets a deadline and a refusal naming the agent. The `SupervisorClient::call` wait is not this node's; the record is **narrowed** to it at wrap, not deleted |
| `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` | ⚠ During. `AnyBackend` and its five matches leave `backend/mod.rs`; `SpecBackend` goes in its own file. Re-measure at wrap |
| `packages/tddy-coder/docs/code-issues/oversized-file-run.md` | ⚠ During. `create_backend` (~120 lines) and `BUILTIN_BACKEND_AGENT_IDS` leave `run.rs`; `build_spec_backend` goes in its own module. Re-measure at wrap |

## Affected Packages

- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): `SpecBackend {Headless(driver), Acp}`; `AnyBackend`, `SharedBackend::from_any`, `routed.rs`'s enum impl and `model_catalog::build_backend` deleted; `AcpDriver::open_session` deadline
- [ ] [`tddy-coder`](../../../packages/tddy-coder/README.md): `build_spec_backend(registry, id, host)`; `create_backend` and `BUILTIN_BACKEND_AGENT_IDS` deleted; `provider_route_arg.rs` on `SpecBackend`
- [ ] [`tddy-model-registry`](../../../packages/tddy-model-registry/README.md): reserved assistant names come from `reserving_agent_ids` (fed with the registry ids), not `tddy_coder::run::BUILTIN_BACKEND_AGENT_IDS` (`store.rs:672`)
- [ ] `tddy-e2e`, `tddy-livekit`, `tddy-integration-tests`, `tddy-tui-testkit`, `tddy-service`, `tddy-presenter`: `AnyBackend::Stub(..)` → `SharedBackend::from_arc(Arc::new(StubBackend::new()))`

## Responsibility

- `SpecBackend` — one `CodingBackend` built from a spec: `Headless(<#821's generic headless driver>)` or `Acp { command, hooks, deps }` over the shared `AcpDriver`. When a spec has both, the transport preference follows `headless_driver.rs:6-7` ("ACP is preferred wherever a harness offers both") unless the caller names one — confirm at red.
- `SpecBackend::with_provider_route(route)` returns n13's `RouteRefusal` for a spec without `route`.
- `build_spec_backend(registry, id, host)`: `stub` → `StubBackend`; a registry id with a `headless` or `acp` section → `SpecBackend`; anything else is refused listing `agent_registry_cli::backend_agent_ids`. Never falls through to claude.
- `tddy-coder --agent <any registry id>` builds a backend, **`oz` included**, and a dropped-in agent too.
- `tddy-model-registry` refuses an assistant named like any registry agent through `reserving_agent_ids`, fed by `tddy-coder` from the loaded registry; its `tddy-coder` dependency for `BUILTIN_BACKEND_AGENT_IDS` goes.
- A dropped-in ACP agent's `AcpDriver::open_session` has a deadline: an agent that never answers `session/new` (or `session/load`) is refused naming the agent and the deadline.
- Delete `AnyBackend`, `create_backend`, `BUILTIN_BACKEND_AGENT_IDS` (see **Removed**).

## Boundaries

- Does not change the generic headless driver (#821) or the ACP backends' protocol code; it composes them.
- Does not touch the daemon's coding-subagent launcher (`coding_agent_launcher.rs`) or the agentless hand-built ACP path ([2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md](../todo/2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md) — sweep node).
- Does not touch daemon.yaml or any CLI-session path.
- No file split beyond moving new code into its own modules; code-issue records only.

**Stack-wide rules this node keeps:**
- The agent process sees no `TDDY_*` variable (inherits #821's decision for headless; ACP backends already set none).
- No fallback: an unknown `--agent` is refused listing the buildable ids.
- No hangs: the `open_session` deadline.
- Deletions not listed in **Removed** need the developer's approval.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n15` headless-driver (#821) | the generic spec-configured headless `CodingBackend` and its `from_spec` constructor | `SpecBackend::Headless` wraps it | change its spec fields, adapters or deadlines |
| `n13` route-support (#819) | agent-keyed `RouteRefusal`, `RoutedAgent` | `SpecBackend::with_provider_route` returns it | add route bodies |
| `n11` acp-agents (#800) | `AcpAgentCommand`, `AcpHooksAdapter`, `AcpDriver`, ACP sections in the specs | `SpecBackend::Acp` | change the ACP command rendering |
| `n12` agent-registry (#801) | `agent_registry_cli::backend_agent_ids`, `CodingAgentRegistry` | builds from the registry; refusal lists those ids | change which agents are listed |

## Draft PR contract

The first push of this PR publishes: `SpecBackend` and its constructors, `build_spec_backend`, the `open_session` deadline parameter, `reserving_agent_ids` wiring, and the failing tests below. Unimplemented bodies are `// TODO(spec-backend): implement`.

## Green wave

**Wave:** 3 of 5
**Greenable independently:** after #821 is green — `SpecBackend::Headless` runs #821's driver, and C1 runs a turn through it
**Concurrent with:** `feature/agent-tools/cli-dispatch-collapse` (n17)
**Blocks:** `feature/agent-tools/agent-config-removal` (n19: the breaking config removal and the non-optional `coding_agents` land only once every backend, like every launch path, is built from specs)

Real dependency edges, as opposed to the branch line:

    n12 → n13      n13 → n14, n15, n16      n15 → n16      n14 → n17      n17 → n18      n16, n18 → n19

## State A

- `AnyBackend {Claude, ClaudeAcp, Cursor, Codex, CodexAcp, Stub, OpenCode}` with five delegating matches (`backend/mod.rs:42-52,106-170`); `SharedBackend::from_arc` already exists (:57-104).
- `create_backend` (`tddy-coder/src/run.rs:2516-2632`) matches agent ids; `BUILTIN_BACKEND_AGENT_IDS` (:2637-2645) has 7 ids and no `oz`; `oz` is listed by `backend_agent_ids` but not buildable.
- `tddy-model-registry` reads `tddy_coder::run::BUILTIN_BACKEND_AGENT_IDS` (`store.rs:672`); `reserving_agent_ids` (:153) exists for `allowed_agents`.
- `model_catalog::build_backend` (:146-170) has its own string match.
- `AcpDriver::open_session` (`acp_driver.rs:395`) awaits `session/new` with no timeout.
- 21 files outside `backend/mod.rs` name `AnyBackend`, mostly `AnyBackend::Stub` in tests.

## State B

- One spec-built backend type; agents are selected by registry id, never by a Rust match.
- `tddy-coder --agent oz` and `--agent <dropped-in id>` run a workflow.
- No ACP session open waits forever.

## Delta

### New
- `packages/tddy-agent-backend/src/backend/spec_backend.rs`
- `packages/tddy-coder/src/spec_backend_build.rs` (`build_spec_backend`)
### Modified
- `tddy-agent-backend`: `backend/mod.rs`, `routed.rs`, `model_catalog.rs`, `acp_driver.rs`, `lib.rs` re-exports
- `tddy-coder`: `run.rs` (`create_backend_for_args` callers :1394, :1610, :1799, :3478, :3852), `provider_route_arg.rs`, `headless_from_spec.rs`, `acp_from_spec.rs`, in-file tests
- `tddy-model-registry`: `store.rs`, `Cargo.toml` (drop the `tddy-coder` edge if nothing else needs it)
- `AnyBackend::Stub` sites: `tddy-e2e/src/lib.rs`, `tddy-e2e/examples/rpc_demo.rs`, `tddy-livekit/tests/acp_session_livekit.rs`, `tddy-integration-tests/tests/{acceptance_tdd_graph_engine,tool_session_provider_route}.rs`, `tddy-tui-testkit/tests/virtual_tui.rs`, `tddy-service/src/integration_tests.rs`, `tddy-presenter/src/{lib.rs,presenter/presenter_impl.rs}`
- Docs at wrap: `docs/ft/coder/1-OVERVIEW.md` (`--agent`), `packages/tddy-agent-backend/docs/architecture.md`
### Removed
- `AnyBackend` and its five matches, `SharedBackend::from_any`, `AnyBackend::with_provider_route` (`routed.rs`), `preselected_index_for_agent`'s enum use
- `model_catalog::build_backend`
- `tddy-coder` `create_backend`, `BUILTIN_BACKEND_AGENT_IDS`, `create_backend_specialized_agent_tests` (rewritten against `build_spec_backend`)

## Testing Strategy

### Acceptance tests

**`tddy-coder`: `packages/tddy-coder/tests/agent_registry_cli_acceptance.rs`** (extend; `ACME_YAML` :14; own `<tddyhome>/tools` + stub binary)

- [ ] `tddy_coder_agent_builds_a_backend_for_an_agent_declared_only_in_tddyhome_tools_and_runs_a_turn_through_its_adapter`
- [ ] `tddy_coder_agent_oz_builds_its_headless_backend`
- [ ] `an_unknown_agent_is_refused_listing_the_registry_ids`

**`tddy-agent-backend`: `packages/tddy-agent-backend/tests/spec_backend_acceptance.rs`**

- [ ] `a_dropped_in_acp_agent_that_never_answers_session_new_is_refused_at_its_deadline_naming_the_agent`
- [ ] `a_spec_backend_without_a_route_section_refuses_a_provider_route`

### Unit / integration tests

- [ ] `tddy-agent-backend`: `SpecBackend` picks the transport per the decided rule when a spec has both sections; `name()` is the spec id; `with_provider_route` on stub is refused.
- [ ] `tddy-model-registry` (`tests/model_registry_reserved_names_unit.rs`): an assistant named like any registry agent is refused; one named like no agent is accepted.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| `--agent` selection | Integration, stub binary | the PRD claim is an end-to-end run |
| ACP deadline | Integration with a silent ACP peer | no hangs |
| Reserved names | Unit | pure check |

## Decisions & Trade-offs

- `SpecBackend` is an enum of two transports, not a trait object per agent: the agent is data.
- Open: transport preference when a spec has both `headless` and `acp` (ACP per `headless_driver.rs:6-7`, or a `--transport` choice).

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-spec-backend-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
