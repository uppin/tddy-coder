# Changeset: A provider route is chosen by the spec's route support, not by a harness enum

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Refactor (with a behaviour gain: a dropped-in agent gets a route)
**Complexity**: 4 (re-checked at red, unchanged: the contract adds `product` and the rename but no new crate, proto or concurrency) — five crates (`tddy-agent-backend`, `tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-daemon`); a cross-crate move of n12's published types (kept by re-export); a spec-vocabulary rename in four built-in YAMLs; ~6 route suites rewritten. Not 5: no proto, no concurrency, no data migration
**Stack:** `#agent-tools 13/19` — branch `feature/agent-tools/route-support`, base `feature/agent-tools/agent-registry` (#801)
**PR:** [#819](https://github.com/uppin/tddy-coder/pull/819) (draft)
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
- `RoutedAgent { id, product, support, key_env }` (owned) is what a route body knows about an agent, built from its spec by `routed_agent(&CodingAgentSpec)`. `product` is the new **`coding_agent.product`** spec key (`product: Claude Code`), read through `CodingAgentSpec::product()`, which answers the agent id when the key is absent. Every built-in spec declares it.
- The route body is chosen by **(shape, provider served)**: `(env, anthropic)` → the Anthropic-env body (today's `claude_code_route`), `(env, meta)` → the key-only body writing the spec's `route.key_env`, `(toml_args, openai_responses)` → today's `codex_route`, `(json_config_file, openai_responses | openai_chat)` → today's `opencode_route`. `bodies_accept(shape)` returns the providers a body exists for.
- **Shapes are renamed after the body format they are**, never after an agent: `args` → `toml_args` (`-c key=value` TOML overrides), `config_file` → `json_config_file`; `env` stays. Function names follow (`claude_code_route` → `anthropic_env_route`, `codex_route` → `toml_args_route`, `opencode_route` → `json_config_file_route`, `muse_route` → `key_env_route`). An override file using an old word is refused at load naming the new one.
- `RouteRefusal` variants carry the agent id and product name, not a `Harness`. A refusal's text names the **product only**, exactly as the `Harness` refusals did: `Acme Code cannot be pointed at another provider; it always runs on its own vendor`; a spec with no `product` shows its id in its place. `NotYetRoutable` keeps today's `{product} ({id}) cannot run on a provider account in this build yet`. The existing `Warp cannot be pointed…` pins (`tddy-agent-backend/tests/warp_cli_acceptance.rs:189`, `tddy-session-lifecycle/tests/warp_cli_session.rs:455`) stand unchanged.
- `opencode.yaml`'s `route.key_env` is `OPENCODE_PROVIDER_KEY`, not `TDDY_OPENCODE_PROVIDER_KEY`: the agent process sees no `TDDY_*` route variable. The route file is otherwise byte-identical (`OPENCODE_ROUTE_KEY_ENV` is deleted, and the body writes `{env:<route.key_env>}`).
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
- The agent process sees no `TDDY_*` route variable: `opencode.yaml`'s `route.key_env` is renamed to `OPENCODE_PROVIDER_KEY` here (**settled 2026-11-05**).
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

The contract commit publishes **the surface only** — the `Harness`-keyed entry points and every caller of them are left byte-untouched, and `/green` moves the callers and deletes `Harness` in one go:

| Symbol | Where | State |
|---|---|---|
| `ProviderKind`, `RouteSupport` (moved verbatim) | `tddy-agent-backend/src/provider_route/support.rs`, re-exported at `provider_route::*` and by `tddy_coding_agent_specs` under n12's paths | implemented |
| `RouteShape::{Env, TomlArgs, JsonConfigFile}` (YAML `env`/`toml_args`/`json_config_file`; the three built-in YAMLs use the new words) | `support.rs` | implemented; the old-word refusal is `TODO(route-support)` |
| `RoutedAgent { id, product, support, key_env }` | `support.rs` | implemented |
| `bodies_accept(RouteShape) -> &'static [ProviderKind]` | `support.rs` | `TODO(route-support)` |
| `by_spec::RouteRefusal` (agent-keyed; `Display` names the product, as the `Harness` text did) | `tddy-agent-backend/src/provider_route/by_spec.rs` | implemented |
| `by_spec::{route_supported, agent_routable, resolve_route}(&RoutedAgent, …)` | `by_spec.rs` | `TODO(route-support)` |
| `CodingAgentSection.product` | `tddy-coding-agent-specs/src/spec.rs` | implemented (parsed); the built-in YAMLs do not declare it yet |
| `CodingAgentSpec::product()`, `routed_agent(&CodingAgentSpec) -> RoutedAgent` | `tddy-coding-agent-specs/src/spec.rs` | `TODO(route-support)` |

At green, `by_spec`'s functions and `RouteRefusal` take the names of the `Harness`-keyed ones at `provider_route` (which are deleted), so a child imports `tddy_agent_backend::provider_route::{RoutedAgent, RouteRefusal, resolve_route}`. A child that compiles against the contract commit imports them from `provider_route::by_spec`.

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
- `tddy-coding-agent-specs`: `src/route.rs` (re-export), `src/spec.rs` (`routed_agent`, `product`), `src/lib.rs`; `specs/{codex,codex-acp,opencode}.yaml` (`shape` words); every built-in spec (`product:`); `opencode.yaml` (`route.key_env: OPENCODE_PROVIDER_KEY`)
- `OPENCODE_PROVIDER_KEY` rename also reaches `tddy-agent-launch/src/{sandboxed_cli_harness_tests,session_provider_route_tests,cli_route_launch_tests}.rs`, `tddy-integration-tests/tests/opencode_acp_backend.rs`, `tddy-session-lifecycle/tests/sandboxed_cli_harness_acceptance.rs:309`
- `tddy-agent-launch`: `harness_from_spec.rs`, `session_provider_route.rs`, `cli_route_launch.rs`, `tool_session_route.rs`, `harness_profile_start.rs`, the five claude route sites
- `tddy-session-lifecycle`: `coding_agent_launcher.rs:67,1272`
- `tddy-daemon`: `provider_route_support.rs`, `account_linking.rs:277` (passes the loaded registry), `runtime/mount_accounts.rs` (hands `accounts_service_over` the registry `mount_host.rs` loads — `HostCodingAgents::agents_shared()`)
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

Written and verified failing (2026-11-05); ✅ approved by the developer 2026-11-05 with one change — refusals name the product only (applied, re-verified).

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/yaml_only_agent_acceptance.rs`** (fixture `tests/agent_registry_support/mod.rs`, extended with `a_registry_daemon().with_tool(..).with_profile(..).with_a_meta_account().build()`, `acme_yaml_declaring(lines)` and a stub that reports `ANTHROPIC_{BASE_URL,AUTH_TOKEN,MODEL}`)

- [x] `a_new_agent_dropped_into_tddyhome_tools_with_an_anthropic_env_route_starts_on_a_provider_account_with_the_accounts_endpoint_in_its_environment`
- [x] `a_new_agent_without_a_route_section_is_refused_on_a_provider_account_naming_the_agent_and_spawns_nothing` — pins `InvalidArgument` and `Acme Code cannot be pointed at another provider; it always runs on its own vendor`
- [x] `a_harness_profile_naming_a_dropped_in_agent_starts_its_session_type` — **moved here** from `harness_clis_from_spec_acceptance.rs`: its stub binary and dropped-in tool file are this fixture's

**`tddy-coding-agent-specs`: `packages/tddy-coding-agent-specs/tests/route_body_drift_acceptance.rs`**

- [x] `every_built_in_specs_route_providers_are_ones_its_route_body_accepts_and_resolves` — **renamed** from `…are_exactly_what_its_route_body_accepts`: `bodies_accept(env)` is `[anthropic, meta]` while Claude declares only `[anthropic]`, so the claim is "a subset that resolves", pinned as a literal table
- [x] `an_override_using_a_removed_route_shape_word_is_refused_at_load_naming_the_new_word`
- [x] `every_built_in_spec_declares_the_product_its_route_refusals_name` — **new**, decision 2
- [x] `an_agent_without_a_route_is_refused_naming_the_product_its_spec_declares` — **new**, decision 2 (`Cursor cannot be pointed…`)

**`tddy-daemon`: `packages/tddy-daemon/tests/provider_route_listing_acceptance.rs`** (real `runtime::build`, stub GitHub, a Meta and an OpenAI-compatible account)

- [x] `list_provider_routes_for_a_dropped_in_agent_offers_the_accounts_its_spec_providers_serve`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/harness_clis_from_spec_acceptance.rs`** — regression, decision 1

- [x] `a_routed_opencode_start_writes_the_same_route_file_with_its_key_in_opencode_provider_key` — **new**: the host route file is byte-identical apart from `{env:OPENCODE_PROVIDER_KEY}`, the key is in `OPENCODE_PROVIDER_KEY`, and `TDDY_OPENCODE_PROVIDER_KEY` is unset
- [x] The existing routed opencode cases (`a_harness_profile_naming_opencode_starts_from_the_spec`, `jailed::host_and_jailed_opencode_cli_…`, `jailed::each_jail_reaches_…`) now read the key from `OPENCODE_PROVIDER_KEY`, and so does `tddy-coding-agent-specs/tests/harness_specs_acceptance.rs` `opencode_yaml_requires_a_route_in_a_jail_…` (`key_env`); the muse cases are unchanged and pass

### Unit / integration tests

Written and verified failing at the contract commit (2026-11-05), each on a `TODO(route-support)` body of this node's surface:

**`tddy-agent-backend/tests/route_by_spec_unit.rs`** (new; the `Harness`-keyed `provider_route_acceptance.rs` is left for green to fold in or delete with `Harness`)
- [x] `a_shapes_bodies_accept_the_providers_a_body_exists_for` (3 cases)
- [x] `route_supported_answers_the_agents_declared_providers_against_the_account` (9 cases: the provider ↔ protocol and Meta ↔ preset truth table, refusal texts naming the product)
- [x] `an_agent_that_declares_no_provider_is_refused_by_both_entry_points_carrying_its_id`
- [x] `a_declared_pair_no_body_serves_is_not_yet_routable_and_names_the_product_and_id` (`(toml_args, anthropic)`)
- [x] `every_agent_a_body_serves_is_routable`
- [x] `an_env_route_to_meta_hands_the_key_in_the_specs_key_env_and_nothing_else`
- [x] `an_env_route_to_meta_for_an_agent_without_a_key_env_is_refused_as_not_yet_routable` — no fallback variable for the key
- [x] `an_anthropic_env_route_is_byte_identical_to_claude_codes_harness_route`
- [x] `a_toml_args_route_is_byte_identical_to_codexs_harness_route`
- [x] `a_json_config_file_route_reads_its_key_from_the_specs_key_env`

**`tddy-coding-agent-specs/tests/routed_agent_unit.rs`** (new)
- [x] `a_spec_declaring_a_product_is_named_by_it`
- [x] `a_spec_declaring_no_product_is_named_by_its_id`
- [x] `a_routed_agent_carries_the_specs_id_product_route_support_and_key_env`
- [x] `a_spec_without_a_route_is_a_routed_agent_with_no_providers_named_by_its_id`
- [x] `a_removed_shape_word_is_refused_naming_the_word_that_replaced_it`

Still to be written at green, with the callers they exercise (those callers are untouched by the contract commit):
- [x] `tddy-daemon` (`provider_route_support.rs` inline): `HarnessRouteSupport` resolves `claude-cli` and `claude` to the same support; an unknown id is refused.
- [x] `tddy-agent-launch` (`session_provider_route_tests.rs`): `CliSessionType` carries its spec's support; a vault default stored as `claude-cli` and one stored as `claude` both apply.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Dropped-in agent route | Integration, stub binary | the claim is a real start with the account's endpoint in the environment |
| Body selection | Unit truth table | pure data |
| Spec ↔ body drift | Acceptance over the built-in registry | catches a spec declaring a provider no body serves |

## Decisions & Trade-offs

- **Placement: `tddy-agent-backend`, re-exported by specs** (discovery option A). No new crate, no new edge; same precedent as the adapters the specs name. Rejected: `tddy-actions` (coding-agent vocabulary in the generic tool crate), a new `tddy-route-support` crate (worth it only if `tddy-accounts` moves off string keys).
- **Clash: alias at the use site** rather than renaming n12's published struct or the accounts trait.
- **Shape renames are breaking for override files**; refused loudly at load, named in the stack's changelog entry (n19). ✅ **Settled 2026-11-05** (developer): `args` → `toml_args`, `config_file` → `json_config_file`, an old word refused at load naming the new one.
- ✅ **Settled 2026-11-05** (developer): `product` is a new `coding_agent.product` spec key, the operator-facing name route refusals use, declared in every built-in spec; absent, the agent id is used. At the 6e review the developer settled that refusals name the **product only**, not `{product} ({id})` — the `Warp cannot be pointed…` text and its pins stand.
- ✅ **Settled 2026-11-05** (developer): `opencode.yaml`'s `route.key_env` is `OPENCODE_PROVIDER_KEY`; the agent process sees no `TDDY_*` route variable, and routed opencode keeps byte-identical route files apart from the variable name.
- **Green (2026-11-05):** `by_spec` is a private module re-exported at `provider_route` (children import `tddy_agent_backend::provider_route::{RoutedAgent, RouteRefusal, resolve_route, …}`). The shared launch fns take `&RoutedAgent`, the claude sites a lazy `RouteAgent::OnHost` (a host with no coding agents refuses only when an account is chosen). String ids resolve `registry.get(id)` then `for_session_type(id)`, so `claude` and `claude-cli` name one spec. `HarnessRouteSupport::new(Option<Arc<CodingAgentRegistry>>)` reads the registry `mount_host` loaded (`DaemonSessionHost::coding_agents()`). `harness_profile_start` accepts a non-`-cli` name only when its spec has an ACP or headless section.
- **Contract commit leaves the `Harness` path untouched** (coordinator, 2026-11-05): the new entry points and the agent-keyed `RouteRefusal` are published in `provider_route::by_spec` so nothing of the parent's code churns twice; `/green` promotes them to `provider_route` and deletes `Harness`.

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-route-support-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (approved 2026-11-05: refusals name the product only)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
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
- ✅ **Resolved at green** (duplicate `--model`): `cli_route_launch::routed_launch_model` blanks the argv's model when the route's own arguments name one (`--model` or `-m`); pinned by `a_routed_launch_whose_route_names_the_model_leaves_the_argv_without_one`. Original note: a dropped-in env/anthropic agent whose own argv carries `--model {model}` gets the Anthropic-env body's `--model` too (`cli_route_launch` appends route args; `launch_model_for` keeps the model for an env route). The acceptance tests do not pin argv; decide at green whether the body's `--model` is dropped when the spec's argv names the model.
- ✅ **Resolved at green** (jailed key): the route's secret env travels in the jail's `grants.env`, not the `session.env` that line 341 filters; with the key renamed to `OPENCODE_PROVIDER_KEY` the jailed stub reads it (`sandboxed_cli_harness_acceptance`). Original note: The jail filters `TDDY_*` out of the agent's env (`jail_launch.rs:341`) and hands them to the MCP server (`:120`): check at green how the renamed opencode key reaches a jailed agent.
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
