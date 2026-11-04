# Initial discovery: route support chooses the route body (#agent-tools 13/19)

**Created:** 2026-11-04
**Changeset:** [2026-11-04-agent-tools-route-support.md](./2026-11-04-agent-tools-route-support.md)

Read-only discovery at base `0dbba9195` (`feature/agent-tools/agent-registry`, #801). Exploration 1 is the whole-node discovery, copied verbatim. Node references in it (`n14`, `n15`) predate the final decomposition: the CLI launch work is now `feature/agent-tools/cli-spec-features` (n14) and `feature/agent-tools/cli-dispatch-collapse` (n17); the backend work is `feature/agent-tools/headless-driver` (n15) and `feature/agent-tools/spec-backend` (n16); the config removal is `feature/agent-tools/agent-config-removal` (n19).

## Exploration 1

### n13 route-support — discovery (read-only, base `0dbba9195`, `feature/agent-tools/agent-registry`)

Scope: move `RouteSupport`/`ProviderKind`/`RouteShape` where `tddy-agent-backend` can read them,
select route bodies from the spec's route support, delete `provider_route::Harness`, pin spec
providers against bodies, give a dropped-in YAML agent a route.
Sources: n12 changeset (the `feature/agent-tools/agent-registry` changeset (#801) § Removed,
"`RouteSupport` replaces `provider_route::Harness`'s matrix"), todo
`docs/dev/todo/2026-11-04-provider-route-harness-enum-remains.md`.

#### 1. State A

##### 1.1 Crate edges today (`[dependencies]`, tddy-* only)

| Crate | Depends on | Relevant fact |
|---|---|---|
| `tddy-coding-agent-specs` | `tddy-actions`, `tddy-agent-backend` | uses `adapters::{AcpHooksAdapters, AdapterRegistry, LaunchAdapters}`, `AcpAgentCommand` (`src/spec.rs:11-12`), `claude_hooks` (`src/slots/{mod,hooks}.rs`), `SessionMode` (`src/headless.rs:18`), `McpServerSpec` (`src/slots/acp.rs:6`) |
| `tddy-agent-backend` | `tddy-workflow`, `tddy-session-store`, `tddy-toolcall` | **no** `tddy-actions`, **no** specs. `pub mod provider_route` at `src/lib.rs:15` |
| `tddy-actions` | `tddy-task` | generic tool defs (`ToolDef`, `RunSection`, `HomeKind`) — no coding-agent vocabulary |
| `tddy-core` | re-exports `pub use tddy_agent_backend::*` (`src/lib.rs:7`) | so every `tddy_core::provider_route::Harness` import is the backend's |
| `tddy-accounts` | own trait `RouteSupport` (`src/provider_account.rs:431`) | **name clash** with specs' `struct RouteSupport`; string-keyed (`harness: &str`) |

Cycle: `agent-backend → specs` is impossible (specs already → agent-backend). Therefore the route
types must leave `tddy-coding-agent-specs` for agent-backend to consume them.

##### 1.2 The route types (live in specs)

| Symbol | Location |
|---|---|
| `enum ProviderKind {Anthropic, OpenaiResponses, OpenaiChat, Meta}` + `as_str`, `Display` | `packages/tddy-coding-agent-specs/src/route.rs:15-46` |
| `enum RouteShape {Env (default), Args, ConfigFile}` | `route.rs:49-62` |
| `struct RouteSupport {providers, shape}` + `is_routable` | `route.rs:66-77` |
| re-export | `src/lib.rs:25` |
| `RouteSlot.{required_in_jail, config_env, files_dir, key_env, providers, shape}` | `src/spec.rs:445-469` |
| `CodingAgentSpec::route_support()` | `src/spec.rs:226-234` |
| reader: catalog | `packages/tddy-daemon-rpc/src/catalog/agent_rows.rs:47` |
| tests | `tests/agent_registry_unit.rs:16,235-258`; `tests/harness_specs_acceptance.rs:11-12,54-55,97-98` |

Built-in spec declarations (already filled at n12 green):

| Spec | `route.providers` | `shape` | other route keys |
|---|---|---|---|
| claude.yaml:44-46, claude-acp.yaml:51-53 | `[anthropic]` | env | — |
| codex.yaml:53-55, codex-acp.yaml:54-56 | `[openai_responses]` | args | — |
| opencode.yaml:61-67 | `[openai_responses, openai_chat]` | config_file | `required_in_jail`, `config_env: OPENCODE_CONFIG`, `files_dir: provider-route`, `key_env: TDDY_OPENCODE_PROVIDER_KEY` |
| muse.yaml:54-58 | `[meta]` | env | `required_in_jail`, `key_env: META_API_KEY` |
| cursor, warp, oz | no `route` | — | not routable |

Duplicated constants a spec already declares: `MUSE_ROUTE_KEY_ENV = "META_API_KEY"` (`tddy-agent-backend/src/cli_harness_muse.rs:19`), `OPENCODE_ROUTE_KEY_ENV` (`provider_route.rs:441`).

##### 1.3 `provider_route::Harness` (`packages/tddy-agent-backend/src/provider_route.rs`, 502 lines)

| Item | Line | What |
|---|---|---|
| `enum Harness` (11 variants) | 22-45 | Claude, ClaudeAcp, ClaudeCli, Codex, CodexAcp, OpenCode, OpenCodeCli, MuseCli, WarpCli, Cursor, CursorCli |
| `from_agent_or_session_type` / `id` | 51, 57 | string ↔ enum over `IDS` |
| `IDS` table + `TODO(agent-registry)` | 66-81 | 11 ids (session types *and* agent ids) |
| `ALL` | 83-95 | |
| `product()` | 98-108 | operator-facing name in refusals |
| `RouteRefusal {HarnessCannotBeRouted, ProtocolNotServed, PresetNotAccepted, NotYetRoutable}` | 179-191, Display 193-219 | every variant carries `Harness` |
| `route_supported(harness, preset, protocols)` — the matrix | 224-251 | anthropic-messages / openai-responses / openai-responses∨chat / preset meta / cannot route |
| `route_body(harness)` | 285-296 | Claude* → `claude_code_route`; OpenCode* → `opencode_route`; Codex* → `codex_route` (`provider_route/codex.rs:10`); MuseCli → `muse_route`; Cursor*/Warp → refuse |
| `harness_routable` / `resolve_route` | 300, 305 | public entry points |
| bodies | `opencode_route` 330, `muse_route` 357, `claude_code_route` 374, `codex_route` codex.rs:10 (also `NotYetRoutable` for an unnamed preset, codex.rs:25-27) |

Note: shape alone does not pick a body. `Env` is both Claude Code (`ANTHROPIC_*` env + `--model`) and
Muse (key only in `key_env`); `Args` is Codex's `-c model_provider=…` TOML syntax; `ConfigFile` is
OpenCode's JSON. Body = f(shape, provider served).

##### 1.4 Every reader of `Harness` / the matrix (production)

| File:line | Use |
|---|---|
| tddy-agent-backend/src/cli_harness.rs:24,89,101,113 | `CliHarness.route_harness` field (legacy table, `TODO(harness-clis)`) |
| tddy-agent-backend/src/cli_harness_muse.rs:13,61; cli_harness_warp.rs:22,62 | same field |
| tddy-agent-backend/src/backend/routed.rs:10,27-29 | `AnyBackend::Cursor` refusal names `Harness::Cursor` |
| tddy-agent-backend/src/provider_route/codex.rs:4,14 | `route_supported(harness, …)` |
| tddy-agent-launch/src/harness_from_spec.rs:21,94,192 | `CliSessionType.route: Option<Harness>` from `from_agent_or_session_type(session_type)` |
| tddy-agent-launch/src/session_provider_route.rs:17,77-117,224-230 | `resolve_session_route(access, Harness, …)`, `route_on_account`, `resume_route(impl Into<Option<Harness>>)`, `no_provider_route_serves_the_agent` + `TODO(agent-registry)` |
| tddy-agent-launch/src/cli_route_launch.rs:7,14,101,131 | `route_harness: impl Into<Option<Harness>>` |
| tddy-agent-launch/src/claude_cli_spawn.rs:35,413 | `start_route(…, Harness::ClaudeCli, …)` |
| tddy-agent-launch/src/cli_branch_starts.rs:29,49,110 | `Harness::ClaudeCli` |
| tddy-agent-launch/src/svc_resume_claude_cli_session.rs:24,297; svc_resume_sandboxed_claude_cli_session.rs:5,51 | `Harness::ClaudeCli` |
| tddy-agent-launch/src/tool_session_route.rs:9,37 | tool-session agent id → Harness |
| tddy-agent-launch/src/harness_profile_start.rs:4,22-39 | profile harness → (session_type \| agent) by matching variants; also `spec_for_session_type` via `profile_spec_id` (todo `2026-11-04-spec-for-session-type-is-unused.md`) |
| tddy-session-lifecycle/src/coding_agent_launcher.rs:67,1272 | subagent def `harness` → Harness |
| tddy-daemon/src/provider_route_support.rs:5-26 | `HarnessRouteSupport` (unit struct) implements `tddy_accounts::RouteSupport`; wired at `tddy-daemon/src/account_linking.rs:277` |
| tddy-coder/src/provider_route_arg.rs:10,40 | via `AnyBackend::with_provider_route` (n15 deletes AnyBackend) |

Counts: `rg` of `Harness::<variant>|from_agent_or_session_type|harness_routable|route_supported(|resolve_route(` → **32 files**
(production 21, test 11). `provider_route.rs` itself 46 hits.

##### 1.5 Tests that touch it

| Suite | Refs | Notes |
|---|---|---|
| tddy-agent-backend/tests/provider_route_acceptance.rs | 27 (9 tests) | matrix + bodies per variant |
| tddy-agent-backend/tests/codex_route_acceptance.rs | 4 | |
| …/muse_cli_acceptance.rs | 4 | asserts `route_harness == Harness::MuseCli` (:32) |
| …/opencode_route_acceptance.rs | 3 | |
| …/warp_cli_acceptance.rs | 3 | `route_harness` (:42), refusal (:174-182) |
| …/cli_harness_acceptance.rs | 2 | `route_harness` (:47) |
| tddy-agent-launch/src/session_provider_route_tests.rs | 20 (24 tests) | in-crate |
| tddy-agent-launch/src/default_route_tests.rs:8,70,95; tool_session_route_tests.rs:149 | 3 + 1 | |
| tddy-daemon/src/provider_route_support.rs (inline) | ~10 | string ids `"cursor-cli"`, `"claude-cli"`, `"muse-cli"`, `"stub"` |
| tddy-accounts/tests/provider_routes_acceptance.rs, provider_account_defaults_acceptance.rs | fake `RouteSupport` | string-keyed, unaffected |
| tddy-session-lifecycle/tests/harness_clis_from_spec_acceptance.rs (9 tests) + harness_clis_support/mod.rs | Meta + OpenAI vault accounts | the integration fixture for routed starts |

Gate scan: `no_agent_name_gates` lists `provider_route.rs ×5` (n12 changeset § Red baseline).

#### 2. Seams and order (tree compiles at each step)

1. **Move types down.** Put `ProviderKind`, `RouteShape`, `RouteSupport` in agent-backend
   (`provider_route/support.rs`) and re-export from specs (`pub use tddy_agent_backend::provider_route::{…}` in
   `specs/src/route.rs`) — every `tddy_coding_agent_specs::ProviderKind` path keeps compiling. Zero consumer edits.
2. **Agent-keyed refusals.** `RouteRefusal` variants carry `agent: String` (+ product) instead of `Harness`.
   Update Display; codex.rs; routed.rs (one arm).
3. **Body by support.** Add `resolve_route_for(&RoutedAgent, &RouteTarget)` / `agent_routable(&RoutedAgent)`
   alongside the old fns; old fns delegate through a `Harness → RoutedAgent` shim (built-in table) so callers still compile.
4. **Launch callers.** `CliSessionType.route: Option<Harness>` → `route: RouteSupport` (+ key_env) from
   `spec.route_support()`; `resolve_session_route` / `route_on_account` / `resume_route` / `start_route` /
   `cli_route_launch` take `&RoutedAgent`. Claude call sites (`Harness::ClaudeCli` ×5) take the `claude` spec's
   support — they already hold `HostCodingAgents`.
5. **String-keyed callers.** `tool_session_route.rs`, `harness_profile_start.rs`, `coding_agent_launcher.rs:1272`,
   `tddy-daemon/provider_route_support.rs`: resolve the id through `CodingAgentRegistry` (`get(id)` for an agent id,
   `for_session_type(st)` for `<id>-cli`). `HarnessRouteSupport` gains a registry field (the daemon builds it in
   `account_linking.rs:277`; must receive the same registry `mount_host.rs:263` loads).
6. **Delete** `Harness`, `IDS`, `ALL`, `from_agent_or_session_type`, `CliHarness.route_harness` (+ its 3 test asserts),
   the shim of step 3; rewrite the 6 backend suites against `RouteSupport`.

Blocking edges: none upstream; `CliHarness` table deletion itself is `TODO(harness-clis)` — only its
`route_harness` field goes here. `tddy-daemon` → registry wiring order: `HarnessRouteSupport` is built in
`account_linking.rs`, must be after the registry load (check `runtime` ordering at green).

#### 3. Proposed owned API surface

**Placement decision (developer call):**

| Option | Edges | Verdict |
|---|---|---|
| A. `tddy-agent-backend::provider_route::{ProviderKind, RouteShape, RouteSupport}`, re-exported by specs | none new | **Recommended**: no new crate, spec-vocabulary sits beside the bodies that consume it (same precedent: adapters named by specs live in agent-backend). Cost: agent-backend is already oversized (`oversized-file-backend-mod.md`) — put it in a new file, not `backend/mod.rs` |
| B. `tddy-actions` | agent-backend gains `tddy-actions`+`tddy-task` | Rejected: coding-agent vocabulary in the generic tool crate; widens agent-backend's graph |
| C. new `tddy-route-support` (serde only) | specs → it, agent-backend → it | Clean but a new workspace member for 3 enums; worth it only if `tddy-accounts` should also consume `ProviderKind` (it is string-keyed today) |

Rename hazard: `tddy_accounts::RouteSupport` (trait) vs `RouteSupport` (struct); `tddy-daemon/provider_route_support.rs`
imports both. Alias at the use site, or rename the struct `AgentRouteSupport` (would touch n12's published name — avoid).

```rust
// tddy-agent-backend/src/provider_route/support.rs  (moved verbatim from specs/src/route.rs)
pub enum ProviderKind { Anthropic, OpenaiResponses, OpenaiChat, Meta }
pub enum RouteShape { Env, Args, ConfigFile }
pub struct RouteSupport { pub providers: Vec<ProviderKind>, pub shape: RouteShape }

// tddy-agent-backend/src/provider_route.rs
/// What a route body needs to know about one agent; built from its spec.
pub struct RoutedAgent<'a> {
    pub id: &'a str,             // agent id, named in refusals
    pub product: &'a str,        // operator-facing name (spec `summary` head, or a new `coding_agent.product`)
    pub support: &'a RouteSupport,
    pub key_env: Option<&'a str>, // spec `route.key_env` (replaces MUSE_ROUTE_KEY_ENV / OPENCODE_ROUTE_KEY_ENV copies)
}
pub enum RouteRefusal {
    AgentCannotBeRouted { agent: String, product: String },
    ProtocolNotServed { agent: String, product: String, needs: &'static str },
    PresetNotAccepted { agent: String, product: String, preset: String },
    NotYetRoutable { agent: String, product: String },   // declared combination no body serves
}
pub fn route_supported(agent: &RoutedAgent<'_>, preset: &str, protocols: &[&str]) -> Result<(), RouteRefusal>;
pub fn agent_routable(agent: &RoutedAgent<'_>) -> Result<(), RouteRefusal>;
pub fn resolve_route(agent: &RoutedAgent<'_>, target: &RouteTarget) -> Result<HarnessRoute, RouteRefusal>;
/// The (shape, provider) pairs a body exists for — what the drift test pins each spec against.
pub fn bodies_accept(shape: RouteShape) -> &'static [ProviderKind];
```

Body table (replaces `route_body`, 285-296): `(Env, Anthropic)` → `claude_code_route`; `(Env, Meta)` →
`key_only_route(key_env)`; `(Args, OpenaiResponses)` → `codex_route`; `(ConfigFile, OpenaiResponses|OpenaiChat)` →
`opencode_route`; empty providers → `AgentCannotBeRouted`; any other declared pair → `NotYetRoutable` (refusal,
never a vendor run). Matrix: provider ↔ protocol (`anthropic-messages`, `openai-responses`, `openai-chat`), `Meta` ↔ preset `meta`.

Launch side (tddy-agent-launch): `resolve_session_route(access, &RoutedAgent, sources, model)`,
`route_on_account(…, &RoutedAgent, …)`, `resume_route(access, Option<&RoutedAgent>, dir)`;
`CliSessionType.route: Option<Harness>` → `route: RouteSupport` (owned) + `key_env`.
`fn routed_agent(spec: &CodingAgentSpec) -> RoutedAgent<'_>` in specs (or launch).
`HarnessRouteSupport { agents: Arc<CodingAgentRegistry> }` (tddy-daemon).
`harness_profile_start::profile_start(registry, profile)`; delete `spec_for_session_type` here (its last caller) — optional, see §6.

**Removed:** `Harness` (+`IDS`, `ALL`, `id`, `from_agent_or_session_type`, `product`), `harness_routable`,
`CliHarness.route_harness`, `no_provider_route_serves_the_agent` + its `TODO(agent-registry)` (session_provider_route.rs:222-230),
`harness_from_spec.rs:94` TODO, specs `route.rs` bodies (becomes a re-export).

Open developer decisions: (a) placement A/B/C; (b) where `product` comes from (refusal text "Claude Code cannot…"
is not pinned by any Rust test; web mocks only, `CreateSessionProviderAcceptance.cy.tsx:66,168`); (c) whether `Args`/`ConfigFile`
should be renamed to the body they really are (`codex_args`, `opencode_config`) or a named `route.adapter` added, since a
dropped-in agent with `shape: args` gets Codex's `-c` syntax — the honest generalisation is only `Env`.

#### 4. Candidate acceptance tests

| # | Test | File | Fixture |
|---|---|---|---|
| A1 | `a_new_agent_dropped_into_tddyhome_tools_with_an_anthropic_env_route_starts_on_a_provider_account_with_the_accounts_endpoint_in_its_environment` | `packages/tddy-session-lifecycle/tests/yaml_only_agent_acceptance.rs` (new case) | `agent_registry_support/mod.rs` `a_daemon_with_tools` + a vault account (borrow from `harness_clis_support` `ADaemonSpec`); stub reports env |
| A2 | `a_new_agent_without_a_route_section_is_refused_on_a_provider_account_naming_the_agent` (InvalidArgument/FailedPrecondition, nothing spawned) | same | same |
| A3 | `every_built_in_specs_route_providers_are_exactly_what_its_route_body_accepts` (drift test: for each spec with `route`, `providers ⊆ bodies_accept(shape)` and a `RouteTarget` per provider resolves) | `packages/tddy-coding-agent-specs/tests/route_body_drift_acceptance.rs` (specs has a dev-path to agent-backend already) | `CodingAgentRegistry::builtin()` |
| A4 | `list_provider_routes_for_a_dropped_in_agent_offers_the_accounts_its_spec_providers_serve` | `packages/tddy-daemon/tests/` (near `unbundle_endpoint.rs:149`) or tddy-accounts with a registry-backed `RouteSupport` | `with_coding_agents`, two accounts |
| A5 | `a_harness_profile_naming_a_dropped_in_agent_starts_its_session_type` | `harness_clis_from_spec_acceptance.rs` (beside :107) | `harness_clis_support` |
| A6 | regression: existing `harness_clis_from_spec_acceptance` routed opencode/muse cases unchanged (byte-identical route files/env) | — | — |

Unit: `route_supported` per (provider, preset, protocols) truth table (replaces provider_route_acceptance's 27 variant refs);
`(Env,Meta)` body puts the key in the spec's `key_env`; `NotYetRoutable` for `(Args, Anthropic)`; refusal Display names the
agent id; `HarnessRouteSupport` resolves `"claude-cli"` and `"claude"` to the same support and `"stub"` → unknown;
`CliSessionType` carries the spec's support.

#### 5. Risks

- **Hangs:** none — pure data. `HarnessRouteSupport` must not load the registry lazily per call from disk (I/O on the accounts RPC); give it the loaded `Arc`.
- **Fallback creep:** `NotYetRoutable`/`AgentCannotBeRouted` must stay refusals. Do **not** default `RouteShape::Env` +
  anthropic for a spec without `route` (`RouteShape` derives `Default = Env`, `route.rs:49-52`): no `route` section ⇒ not routable (pinned by `agent_registry_unit.rs:253-258`).
  A string id the registry does not know must refuse ("unknown harness"), not resolve to claude.
- **Persisted data:** vault `default_for` metadata stores harness strings (`"claude-cli"`, `"codex"`; `tddy-accounts/src/provider_account.rs:440-463`). Keep accepting both session types and agent ids — no migration.
- **Linux/macOS:** route files in jail (`tddy-sandbox-recipes/src/route_files.rs`, `cli_harness_jail.rs:102-147`) unaffected; files_dir/config_env already spec-read.
- **Proto/web:** none required. `accounts.proto:274` `ListProviderRoutesRequest.harness` stays a string; `catalog.proto` `AgentInfo.route_providers` already published by n12. Refusal message text changes reach the web picker (`harness_reason`) — cypress mocks hard-code strings, not real.
- **Docs at wrap:** `docs/ft/daemon/provider-accounts.md`, `harness-profiles.md`, `opencode-cli-session.md`, `muse-cli-session.md`, `packages/tddy-agent-backend/docs/architecture.md`, `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md` (route section), the n12 PRD `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md` AC "RouteSupport replaces Harness". Delete todo `2026-11-04-provider-route-harness-enum-remains.md` at wrap.

#### 6. Overlap

| With | Edge |
|---|---|
| n14 | n14's claude/cursor paths call `start_route(…, Harness::ClaudeCli, …)` (5 sites). If n13 lands first they become `routed_agent(&claude_spec)`; n14 then folds them into the generic `cli_route_launch` path that already takes the spec. n13 first is cheaper (n14 inherits spec-keyed routing). |
| n15 | `AnyBackend::with_provider_route` (`routed.rs:17-38`) names `Harness::Cursor`; n13 changes it to agent-keyed, n15 deletes it. `tddy-coder` `provider_route_arg.rs` follows. No order force. |
| `spec_for_session_type` todo | only caller is `harness_profile_start::profile_spec_id`, which n13 rewrites — natural place to delete it (+2 unit tests). |

Order n12 → n13 holds; n13 depends on nothing in n14/n15.

#### 7. Complexity

**4** — more than one crate (agent-backend, coding-agent-specs, agent-launch, session-lifecycle, daemon); a cross-crate type
move (contract change of n12's published `RouteSupport` path, kept by re-export); ~6 test suites rewritten. Not 5: no
proto change, no concurrency, no migration, design settled except placement/product-name.

#### 8. Code issues touched

| Record | Claimed |
|---|---|
| `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` (850 prod lines) — only if the types land in `backend/mod.rs`; they should not | unclaimed |
| `packages/tddy-coding-agent-specs/docs/code-issues/oversized-file-spec.md` (656) — `RouteSlot` stays; shrinks nothing | unclaimed |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-claude-cli-session-resume-claude-cli-session.md` — touched at :297 only | unclaimed |

None `Claimed by:` (only claimed records repo-wide are in tddy-code-restructuring, tddy-workflow-recipes, tddy-zed-extensions).
