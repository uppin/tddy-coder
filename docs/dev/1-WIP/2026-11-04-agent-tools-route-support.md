# Changeset: A provider route is chosen by the spec's route support, not by a harness enum

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Refactor (with a behaviour gain: a dropped-in agent gets a route)
**Complexity**: 4 — five crates (`tddy-agent-backend`, `tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-daemon`); a cross-crate move of n12's published types (kept by re-export); a spec-vocabulary rename in four built-in YAMLs; ~6 route suites rewritten. Not 5: no proto, no concurrency, no data migration
**Stack:** `#agent-tools 13/19` — branch `feature/agent-tools/route-support`, base `feature/agent-tools/agent-registry` (#801)
**PRD:** none of its own. It delivers the "`RouteSupport` replaces `provider_route::Harness`" criterion of the registry PRD on `feature/agent-tools/agent-registry`: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`

## Initial Discovery

[2026-11-04-agent-tools-route-support-initial-discovery.md](./2026-11-04-agent-tools-route-support-initial-discovery.md)

State A below is distilled from that file. Grep traces are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-provider-route-harness-enum-remains.md](../todo/2026-11-04-provider-route-harness-enum-remains.md) | ✅ **RESOLVED HERE.** `Harness`, its `IDS` table and `from_agent_or_session_type` are deleted; the body is chosen by (shape, provider served) from the spec |
| [2026-11-04-spec-for-session-type-is-unused.md](../todo/2026-11-04-spec-for-session-type-is-unused.md) | ✅ **RESOLVED HERE.** Its last caller, `harness_profile_start::profile_spec_id`, is rewritten onto `CodingAgentRegistry::for_session_type`, and the table and its two unit tests are deleted |
| [2026-11-03-harness-clis-legacy-cli-harness-removal.md](../todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md) | ⚠ During. Only `CliHarness.route_harness` (and its three test asserts) goes here; the table itself is not this node's |
| `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` | ⚠ During. The moved types go in a new `provider_route/support.rs`, never in `backend/mod.rs`. Re-measure at wrap |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-claude-cli-session-resume-claude-cli-session.md` | ⚠ During. Touched at one call site (`:297`). Restructuring is deferred to the stack's sweep node |

## Affected Packages

- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): `provider_route::{ProviderKind, RouteShape, RouteSupport, RoutedAgent}`, agent-keyed `RouteRefusal`, body table by (shape, provider), `bodies_accept`; `Harness` deleted
- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `route.rs` becomes a re-export; renamed `route.shape` words in `codex*.yaml`, `opencode.yaml`; `routed_agent(&spec)`; `spec_for_session_type` deleted
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): `CliSessionType.route`, `session_provider_route`, `cli_route_launch`, the five `Harness::ClaudeCli` sites, `tool_session_route`, `harness_profile_start`
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): subagent definition `harness` → registry lookup (`coding_agent_launcher.rs:67,1272`)
- [ ] [`tddy-daemon`](../../../packages/tddy-daemon/README.md): `HarnessRouteSupport` reads the loaded registry (`provider_route_support.rs`, `account_linking.rs:277`)

## Responsibility

- `ProviderKind`, `RouteShape` and `RouteSupport` live in `tddy-agent-backend::provider_route` (new file `provider_route/support.rs`) and are re-exported from `tddy_coding_agent_specs` under the paths n12 published, so no consumer import changes. The move is forced by the edge `tddy-coding-agent-specs → tddy-agent-backend`: the reverse edge is a cycle.
- The name clash with the `tddy_accounts::RouteSupport` **trait** is resolved at the one use site that imports both (`tddy-daemon/src/provider_route_support.rs`), by importing the trait under an alias. Neither public name changes.
- `RoutedAgent { id, product, support, key_env }` is what a route body knows about an agent, built from its spec by `routed_agent(&CodingAgentSpec)`.
- The route body is chosen by **(shape, provider served)**: `(env, anthropic)` → the Anthropic-env body (today's `claude_code_route`), `(env, meta)` → the key-only body writing the spec's `route.key_env`, `(toml_args, openai_responses)` → today's `codex_route`, `(json_config_file, openai_responses | openai_chat)` → today's `opencode_route`. `bodies_accept(shape)` returns the providers a body exists for.
- **Shapes are renamed after the body format they are**, never after an agent: `args` → `toml_args` (`-c key=value` TOML overrides), `config_file` → `json_config_file`; `env` stays. Function names follow (`claude_code_route` → `anthropic_env_route`, `codex_route` → `toml_args_route`, `opencode_route` → `json_config_file_route`, `muse_route` → `key_env_route`). An override file using an old word is refused at load naming the new one.
- `RouteRefusal` variants carry the agent id and product name, not a `Harness`.
- A dropped-in YAML agent with a `route` section is routable through the same bodies. A spec with **no** `route` is unroutable and refused (`AgentCannotBeRouted`), never defaulted to `env` + anthropic (`RouteShape`'s serde `Default` stays a parse default only). A declared pair no body serves is `NotYetRoutable`, a refusal.
- String-keyed callers (`tool_session_route.rs`, `harness_profile_start.rs`, `coding_agent_launcher.rs:1272`, `HarnessRouteSupport`) resolve through `CodingAgentRegistry::get(id)` or `for_session_type(st)`. An id the registry does not know is refused ("unknown agent"), never resolved to claude.
- Vault `default_for` metadata keeps matching both id forms (`claude-cli` and `claude`); no stored record is migrated.
- A drift test pins each built-in spec's `route.providers` to `bodies_accept(shape)`.
- Delete `provider_route::Harness` (+ `IDS`, `ALL`, `id`, `product`, `from_agent_or_session_type`), `harness_routable`, `CliHarness.route_harness`, `no_provider_route_serves_the_agent` and its `TODO(agent-registry)`, and `spec_for_session_type`.

## Boundaries

- No change to the claude/cursor dispatch or their launch paths beyond swapping `Harness::ClaudeCli` for the claude spec's `RoutedAgent` at the five sites. Folding them into the generic path is `feature/agent-tools/cli-spec-features` (n14).
- `AnyBackend::with_provider_route` (`backend/routed.rs`) only changes its refusal to agent-keyed; deleting it is `feature/agent-tools/spec-backend` (n16).
- No proto or web change. `ListProviderRoutesRequest.harness` stays a string.
- The `CliHarness` table, route files in the jail (`tddy-sandbox-recipes/src/route_files.rs`) and `files_dir`/`config_env` reading are untouched.
- No file split or restructuring; code-issue records only (sweep node).

**Stack-wide rules this node keeps:**
- The agent process sees no `TDDY_*` variable. `opencode.yaml`'s `route.key_env: TDDY_OPENCODE_PROVIDER_KEY` breaks this today; renaming it (to a non-`TDDY_` name) is in scope here because the key-env body is rewritten here — **open, confirm at red**.
- No fallback: an unknown agent id or a spec without `route` is refused, never routed as claude.
- No hangs: the route path is pure data; `HarnessRouteSupport` gets the already-loaded registry and never loads from disk per RPC.
- Fixtures install coding agents (`with_coding_agents`) and the sandbox RPC bridge; Linux jail tests self-skip without user namespaces.
- Deletions not listed in **Removed** need the developer's approval.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n12` agent-registry (#801) | `ProviderKind`, `RouteShape`, `RouteSupport`, `RouteSlot.{providers, shape, key_env}`, `CodingAgentSpec::route_support`, `CodingAgentRegistry::{get, for_session_type}`, `HostCodingAgents` on the host, `route.providers` in the built-in specs | moves the three types to agent-backend behind a re-export; builds `RoutedAgent` from `route_support()`; resolves string ids through the registry | change the registry's lookup rules, the `AgentInfo.route_providers` field, or the capabilities |
| `n8` harness-clis (#797) | opencode/muse/warp launch from their specs via `cli_route_launch` | passes `&RoutedAgent` instead of `Harness` | change the generic CLI launch |

## Draft PR contract

The first push of this PR publishes: `provider_route/support.rs` with the moved types and the renamed `RouteShape` words; `RoutedAgent`, the agent-keyed `RouteRefusal`, `bodies_accept`, `routed_agent`, the new signatures of `resolve_route`/`route_supported`/`agent_routable`; the failing acceptance tests and drift test below. Unimplemented bodies are `// TODO(route-support): implement`.

## Green wave

**Wave:** 1 of 5 (of the appended nodes n13–n19)
**Greenable independently:** yes — every test needs only this node's surface plus n12's registry, which is published and green-bound on #801
**Concurrent with:** none (n12 is the line below; n14 and n15 both consume this node's `RouteRefusal`/`RoutedAgent`)
**Blocks:** `feature/agent-tools/cli-spec-features` (n14: its claude/cursor routes go through `RoutedAgent`), `feature/agent-tools/headless-driver` (n15: the generic driver's route), `feature/agent-tools/spec-backend` (n16: `SpecBackend::with_provider_route` returns this `RouteRefusal`)

Real dependency edges, as opposed to the branch line:

    n12 → n13      n13 → n14, n15, n16      n15 → n16      n14 → n17      n17 → n18      n16, n18 → n19

## State A

- The route types live in `tddy-coding-agent-specs/src/route.rs:15-77`; `tddy-agent-backend` cannot read them (specs → agent-backend already).
- `provider_route::Harness` (`tddy-agent-backend/src/provider_route.rs:22-108`) enumerates 11 session types and agent ids; `route_supported` (:224-251) is the matrix; `route_body` (:285-296) picks the body by variant; every `RouteRefusal` variant carries a `Harness` (:179-219).
- 21 production files read `Harness`, including `harness_from_spec.rs:94,192` (`CliSessionType.route: Option<Harness>`), `session_provider_route.rs:77-117,224-230`, `cli_route_launch.rs:14,101,131`, five `Harness::ClaudeCli` sites (`claude_cli_spawn.rs:413`, `cli_branch_starts.rs:49,110`, `svc_resume_claude_cli_session.rs:297`, `svc_resume_sandboxed_claude_cli_session.rs:51`), `tool_session_route.rs:37`, `harness_profile_start.rs:22-39`, `coding_agent_launcher.rs:1272`, `tddy-daemon/src/provider_route_support.rs:5-26`.
- `RouteShape::{Env, Args, ConfigFile}`: shape alone does not pick a body (`Env` is both Claude Code and Muse), and `Args`/`ConfigFile` are Codex's and OpenCode's formats under generic names.
- `MUSE_ROUTE_KEY_ENV` (`cli_harness_muse.rs:19`) and `OPENCODE_ROUTE_KEY_ENV` (`provider_route.rs:441`) duplicate `route.key_env` the specs declare.
- `spec_for_session_type` is read only by `harness_profile_start::profile_spec_id`.
- A dropped-in agent with a `route` section is not routable: no `Harness` variant names it.

## State B

- Route bodies are chosen from the spec's (shape, provider served); no agent enumeration exists for routing.
- A dropped-in agent declaring `route: { providers: [anthropic], shape: env }` starts on an Anthropic provider account with the account's endpoint in its environment.
- A spec without `route` is refused on a provider account, naming the agent; nothing is spawned.
- Each built-in spec's providers are pinned against what its body accepts.

## Delta

### New
- `packages/tddy-agent-backend/src/provider_route/support.rs` (moved types, `RoutedAgent`, `bodies_accept`)
- `packages/tddy-coding-agent-specs/tests/route_body_drift_acceptance.rs`
### Modified
- `tddy-agent-backend`: `provider_route.rs` (body table, refusals), `provider_route/codex.rs`, `backend/routed.rs` (refusal only), `cli_harness{,_muse,_warp}.rs` (drop `route_harness`)
- `tddy-coding-agent-specs`: `src/route.rs` (re-export), `src/spec.rs` (`routed_agent`), `src/lib.rs`; `specs/{codex,codex-acp,opencode}.yaml` (`shape` words)
- `tddy-agent-launch`: `harness_from_spec.rs`, `session_provider_route.rs`, `cli_route_launch.rs`, `tool_session_route.rs`, `harness_profile_start.rs`, the five claude route sites
- `tddy-session-lifecycle`: `coding_agent_launcher.rs:67,1272`
- `tddy-daemon`: `provider_route_support.rs`, `account_linking.rs:277` (passes the loaded registry)
- Tests: `tddy-agent-backend/tests/{provider_route,codex_route,opencode_route,muse_cli,warp_cli,cli_harness}_acceptance.rs`; `tddy-agent-launch/src/{session_provider_route_tests,default_route_tests,tool_session_route_tests}.rs`
- Docs at wrap: `docs/ft/daemon/provider-accounts.md`, `harness-profiles.md`, `opencode-cli-session.md`, `muse-cli-session.md`, `packages/tddy-agent-backend/docs/architecture.md`, `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md` (route section)
### Removed
- `provider_route::Harness` with `IDS`, `ALL`, `id`, `product`, `from_agent_or_session_type`; `harness_routable`; `route_body(harness)`
- `CliHarness.route_harness` field
- `session_provider_route::no_provider_route_serves_the_agent` and its `TODO(agent-registry)`; the `harness_from_spec.rs:94` `TODO(agent-registry)`
- `MUSE_ROUTE_KEY_ENV`, `OPENCODE_ROUTE_KEY_ENV` (read from `route.key_env`)
- `tddy_coding_agent_specs::spec_for_session_type` and its two unit tests
- The route type definitions in `tddy-coding-agent-specs/src/route.rs` (the file becomes a re-export)

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/yaml_only_agent_acceptance.rs`** (fixture `tests/agent_registry_support/mod.rs` `a_daemon_with_tools`, plus a vault account as `harness_clis_support` builds one; the stub reports its environment)

- [ ] `a_new_agent_dropped_into_tddyhome_tools_with_an_anthropic_env_route_starts_on_a_provider_account_with_the_accounts_endpoint_in_its_environment`
- [ ] `a_new_agent_without_a_route_section_is_refused_on_a_provider_account_naming_the_agent_and_spawns_nothing`

**`tddy-coding-agent-specs`: `packages/tddy-coding-agent-specs/tests/route_body_drift_acceptance.rs`**

- [ ] `every_built_in_specs_route_providers_are_exactly_what_its_route_body_accepts`
- [ ] `an_override_using_a_removed_route_shape_word_is_refused_at_load_naming_the_new_word`

**`tddy-daemon`: `packages/tddy-daemon/tests/provider_route_listing_acceptance.rs`**

- [ ] `list_provider_routes_for_a_dropped_in_agent_offers_the_accounts_its_spec_providers_serve`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/harness_clis_from_spec_acceptance.rs`**

- [ ] `a_harness_profile_naming_a_dropped_in_agent_starts_its_session_type`
- [ ] Regression: the existing routed opencode and muse cases keep byte-identical route files and environment

### Unit / integration tests

- [ ] `tddy-agent-backend` (`tests/provider_route_acceptance.rs`, rewritten): `route_supported` truth table per (provider, preset, protocols); `(env, meta)` writes the key into the spec's `key_env`; `(toml_args, anthropic)` is `NotYetRoutable`; empty providers is `AgentCannotBeRouted`; every refusal's `Display` names the agent id.
- [ ] `tddy-daemon` (`provider_route_support.rs` inline): `HarnessRouteSupport` resolves `claude-cli` and `claude` to the same support; an unknown id is refused.
- [ ] `tddy-agent-launch` (`session_provider_route_tests.rs`): `CliSessionType` carries its spec's support; a vault default stored as `claude-cli` and one stored as `claude` both apply.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Dropped-in agent route | Integration, stub binary | the claim is a real start with the account's endpoint in the environment |
| Body selection | Unit truth table | pure data |
| Spec ↔ body drift | Acceptance over the built-in registry | catches a spec declaring a provider no body serves |

## Decisions & Trade-offs

- **Placement: `tddy-agent-backend`, re-exported by specs** (discovery option A). No new crate, no new edge; same precedent as the adapters the specs name. Rejected: `tddy-actions` (coding-agent vocabulary in the generic tool crate), a new `tddy-route-support` crate (worth it only if `tddy-accounts` moves off string keys).
- **Clash: alias at the use site** rather than renaming n12's published struct or the accounts trait.
- **Shape renames are breaking for override files**; refused loudly at load, named in the stack's changelog entry (n19).
- Open: where `product` (the operator-facing name in refusals) comes from — the spec `summary` head or a new `coding_agent.product` key. No Rust test pins today's text; decide at red.

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-route-support-initial-discovery.md`)
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
