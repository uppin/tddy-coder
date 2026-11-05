# Changeset: `SpecBackend` replaces `AnyBackend`; `tddy-coder --agent` builds any registry agent

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Refactor (with a behaviour gain: `oz` and dropped-in agents are buildable workflow backends)
**Complexity**: 4 (re-scored at red, 2026-11-05: unchanged) — more than one crate (`tddy-agent-backend`, `tddy-coder`, `tddy-model-registry`, plus `AnyBackend::Stub` test sites in `tddy-e2e`, `tddy-livekit`, `tddy-integration-tests`, `tddy-tui-testkit`, `tddy-service`, `tddy-presenter`); a public-type removal; a new deadline on the ACP session open. Not 5: the backend halves already exist (#821's driver, `AcpDriver`), no proto, no data migration. Red showed it slightly bigger than planned — a generic `SpecAcpBackend` (a dropped-in ACP agent has no bespoke backend to wrap), a `tddy-daemon` wiring line, and `model_catalog::build_backend` turned out to be `tddy-tools list-models`' backend (deferred, see Decisions) — but no new crate edge and no new seam
**Stack:** `#agent-tools 16/19` — branch `feature/agent-tools/spec-backend`, base `feature/agent-tools/headless-driver` (#821)
**PR:** [#822](https://github.com/uppin/tddy-coder/pull/822) (draft)
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
- [ ] [`tddy-model-registry`](../../../packages/tddy-model-registry/README.md): reserved assistant names come from `reserving_coding_agent_ids` (fed with `tddy_coder::agent_registry_cli::reserved_agent_ids`), not `tddy_coder::run::BUILTIN_BACKEND_AGENT_IDS` (`store.rs:672`); its `tddy-coder` dependency goes
- [ ] `tddy-daemon`: `runtime/mount_host.rs` loads the host's coding agents before the model registry and feeds `reserved_agent_ids(&registry)` to `reserving_coding_agent_ids` (one line; the crate already depends on `tddy-coder`)
- [ ] `tddy-e2e`, `tddy-livekit`, `tddy-integration-tests`, `tddy-tui-testkit`, `tddy-service`, `tddy-presenter`: `SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()))` → `SharedBackend::stub()` (the test double this node owns)

## Responsibility

- `SpecBackend` — one `CodingBackend` built from a spec: `Headless(SpecHeadlessBackend)` (#821's generic headless driver) or `Acp(SpecAcpBackend)` — a generic ACP backend (rendered command, named hooks adapter, deps) over the shared `AcpDriver`. **Settled:** when a spec declares both, it runs **headless** unless the session or request asks for ACP (`SpecTransport::chosen`).
- `SpecBackend::with_provider_route(route)` returns n13's `RouteRefusal` for a spec without `route`.
- `build_spec_backend(tools_dir, id, host, SpecBackendRequest { transport, provider_route })`: `stub` → `StubBackend` (a route refused); a registry id with a `headless` or `acp` section → `spec_backend_for` → `SpecBackend`, route applied; an id no agent answers to is refused listing `agent_registry_cli::backend_agent_ids`; a spec with neither section is refused naming the agent. Never falls through to claude. `create_backend_for_args` keeps resolving specialized agent defs first, then wraps the host with the `--*-path` flags (`host_naming_binary`) and calls it.
- `tddy-coder --agent <any registry id>` builds a backend, **`oz` included**, and a dropped-in agent too.
- `tddy-model-registry` refuses an assistant named like any registry agent through `reserving_agent_ids`, fed by `tddy-coder` from the loaded registry; its `tddy-coder` dependency for `BUILTIN_BACKEND_AGENT_IDS` goes.
- A dropped-in ACP agent's `AcpDriver::open_session` has a deadline: an agent that never answers `session/new` (or `session/load`) is refused naming the agent and the deadline.
- Delete `AnyBackend`, `create_backend`, `BUILTIN_BACKEND_AGENT_IDS` (see **Removed**).
- `SharedBackend::stub()` — the test double replacing `AnyBackend::Stub` at every test site.

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

**Parent-green needs (6f).** These tests drive a parent's `todo!()` once this node's own bodies exist; at red they fail on this node's `todo!("spec-backend: …")` first. The real object is kept because the integration *is* the point:

| Test | Reaches | Green after |
|---|---|---|
| `tddy_coder_agent_builds_a_backend_for_an_agent_declared_only_in_tddyhome_tools_and_runs_a_turn_through_its_adapter` | `headless_backend_from_spec`, `SpecHeadlessBackend::invoke`/`name` | n15 (#821) |
| `tddy_coder_agent_oz_builds_its_headless_backend` | `headless_backend_from_spec`, `SpecHeadlessBackend::name` | n15 (#821) |
| `an_agent_declaring_both_headless_and_acp_is_built_headless_when_nothing_asks_for_acp` | `headless_backend_from_spec` | n15 (#821) |
| `a_spec_backend_without_a_route_section_refuses_a_provider_route` | `provider_route::by_spec::agent_routable` | n13 (#819) |

Every other test in this node exercises only this node's surface (and `acp_from_spec`'s existing `acp_launch`, `CodingAgentRegistry`, `StubBackend`, `AcpDriver`, all on master).
## Draft PR contract

Published in commit 2 (unimplemented bodies are `// TODO(spec-backend): implement` + `todo!`):

| Symbol | Where |
|---|---|
| `SpecTransport {Headless, Acp}`, `SpecTransport::chosen(agent, declares_headless, declares_acp, asked)` | `tddy-agent-backend/src/backend/spec_backend.rs:33,57` |
| `SpecBackendRefusal {NoTurnTransport, TransportNotDeclared}` (+ `Display`) | `spec_backend.rs:77` |
| `SpecAcpBackend::{new, with_hooks, with_session_open_deadline, with_provider_route}`, `impl CodingBackend` | `spec_backend.rs:107-211` |
| `SpecBackend {Headless(SpecHeadlessBackend), Acp(SpecAcpBackend)}`, `transport`, `with_provider_route`, `impl CodingBackend` | `spec_backend.rs:217-257` |
| `SharedBackend::stub()` — the `AnyBackend::Stub` replacement | `spec_backend.rs:264` |
| `acp_driver::SESSION_OPEN_DEADLINE` (120 s), `AcpDriver::open_session_within(open, deadline)` | `tddy-agent-backend/src/backend/acp_driver.rs:227,413` |
| re-exports | `tddy-agent-backend/src/lib.rs` |
| `spec_backend_build::{SpecBackendRequest, spec_backend_for, build_spec_backend}` | `tddy-coder/src/spec_backend_build.rs:21,39,56` |
| `agent_registry_cli::reserved_agent_ids(registry)` | `tddy-coder/src/agent_registry_cli.rs:33` |
| `ModelRegistryStore::reserving_coding_agent_ids(ids)` (drops the ids until green) | `tddy-model-registry/src/store.rs:164` |

Green also: `AcpDriver::open_session` delegates to `open_session_within(…, SESSION_OPEN_DEADLINE)`; `SpecAcpBackend` maps a passed deadline to `the ACP agent `<id>` did not open a session within <deadline>; it is refused rather than waited on`.

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
- ~~`model_catalog::build_backend`~~ — deferred, see Decisions & Trade-offs
- `tddy-coder` `create_backend`, `BUILTIN_BACKEND_AGENT_IDS`, `create_backend_specialized_agent_tests` (rewritten against `build_spec_backend`)

## Testing Strategy

### Acceptance tests

Written and failing for this node's own reason (red, 2026-11-05). The user review of 6e was waived by the developer ("finish without stopping").

**`tddy-coder`: `packages/tddy-coder/tests/agent_registry_cli_acceptance.rs`** (extended; `ACME_YAML` :27; own `<tddyhome>/tools` + stub binaries named through each spec's `binary.env`)

- [x] `tddy_coder_agent_builds_a_backend_for_an_agent_declared_only_in_tddyhome_tools_and_runs_a_turn_through_its_adapter` (:221)
- [x] `tddy_coder_agent_oz_builds_its_headless_backend` (:256)
- [x] `an_unknown_agent_is_refused_listing_the_registry_ids` (:270)

**`tddy-agent-backend`: `packages/tddy-agent-backend/tests/spec_backend_integration.rs`** (named `_integration`, not `_acceptance`, per the fluent-tests file-name rule)

- [x] `a_dropped_in_acp_agent_that_never_answers_session_new_is_refused_at_its_deadline_naming_the_agent` (:91; silent shell peer, 1 s deadline, 15 s hang guard)
- [x] `a_spec_backend_without_a_route_section_refuses_a_provider_route` (:123)

### Unit / integration tests

- [x] `tddy-agent-backend` (`tests/spec_backend_unit.rs`): the transport rule — both sections → headless (:65), both + asked ACP → ACP (:74), only ACP (:83), only headless (:92), only headless + asked ACP refused (:101), neither refused (:120); `name()` is the spec id (:139); `transport()` (:151); `SharedBackend::stub()` is named `stub` (:163).
- [x] `tddy-coder` (`tests/spec_backend_build.rs`): the decided rule pinned on a YAML agent with both sections — headless by default (:152), ACP when asked (:164); a PTY-only agent refused naming it (:176); `stub` builds (:192) and refuses a route (:210); `reserved_agent_ids` is every registry id then `stub` (:231).
- [x] `tddy-model-registry` (`tests/model_registry_reserved_names_unit.rs`): a dropped-in registry agent's name is refused (:323), `oz` is refused once the registry declares it (:338), a name no agent has is accepted (:353, guard — passes today). `a_coding_backend_id_is_refused` (:132) now feeds its ids through `reserving_coding_agent_ids`, so it stays green when `BUILTIN_BACKEND_AGENT_IDS` goes.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| `--agent` selection | Integration, stub binary | the PRD claim is an end-to-end run |
| ACP deadline | Integration with a silent ACP peer | no hangs |
| Reserved names | Unit | pure check |

## Decisions & Trade-offs

- `SpecBackend` is an enum of two transports, not a trait object per agent: the agent is data.
- **Settled (orchestrator, 2026-11-05):** a spec declaring both `headless` and `acp` runs **headless** unless the session or request asks for ACP (`SpecBackendRequest.transport`). Pinned by `spec_backend_unit.rs:65,74` and `spec_backend_build.rs:152,164`; documented in `packages/tddy-coding-agent-specs/docs/` at wrap.
- **Settled:** `SpecBackend { Headless(SpecHeadlessBackend), Acp(SpecAcpBackend) }` replaces `AnyBackend`; `tddy-coder --agent <any registry id>` builds one, `oz` included; a dropped-in YAML agent with a `headless` section and a known stream adapter runs a turn.
- **Settled:** `create_backend` and `BUILTIN_BACKEND_AGENT_IDS` are deleted; `tddy-model-registry` is fed the registry's ids through the store's reserved agent ids (`reserving_coding_agent_ids`, a sibling of `reserving_agent_ids` so the refusal keeps saying `is a coding backend`, not `is listed in allowed_agents`).
- **Settled:** `AcpDriver::open_session` gets a deadline (`SESSION_OPEN_DEADLINE`, 120 s; `open_session_within`), covering `initialize` too — "host is mandatory for runners, no hangs".
- **Settled:** an agent whose spec has neither `headless` nor `acp` is refused with a clear error naming it (`SpecBackendRefusal::NoTurnTransport`). No fallback.
- **Settled:** the replacement test double is `SharedBackend::stub()`; tests keep `AnyBackend::Stub` until green swaps every site.
- **Settled:** every ACP agent `--agent` builds — the three builtins too — is a generic `SpecAcpBackend`; the system prompt is merged into the prompt, never dropped, and the hooks adapter's open scope brackets each open (codex-acp's OAuth URL file).
- **Deferred (beyond Removed):** `model_catalog::build_backend` is `tddy-tools list-models`' backend, which `tddy-agent-backend` cannot build from a spec. Green changes its return type to `Box<dyn CodingBackend>` (so `AnyBackend` can go) and marks it `TODO(spec-backend)`; it and the three bespoke ACP backends are recorded in [`2026-11-05-model-listing-and-bespoke-acp-backends-remain.md`](../todo/2026-11-05-model-listing-and-bespoke-acp-backends-remain.md) for the sweep (n20).

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-spec-backend-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer: "finish without stopping", 2026-11-05)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (2026-11-05; scoped gates green)
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
- Baseline (6c, 2026-11-05, clean base `316344ae5`): `cargo build`, `clippy --all-targets -D warnings`, `fmt --check` clean for tddy-agent-backend, tddy-coder, tddy-model-registry. Tests: 329 passed, 63 failed — every failure a parent's red (`todo!` of `route-support` n13 ×20 in `route_by_spec_unit`, `cli-spec-features` n14 ×3 in `conversation_id_adapter_unit`, `headless-driver` n15 ×28 in `headless_protocol_unit` and ×12 in each `spec_headless_backend_acceptance`). Not this node's.
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
