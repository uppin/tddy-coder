# Changeset: Every OpenCode launch carries its route inline, by the spec's `route.shape: inline_config`

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 4 — a new route shape and its body (`tddy-agent-backend`), its load validation and `opencode.yaml` (`tddy-coding-agent-specs`), the daemon ACP planner (`tddy-session-lifecycle`), jail egress reading a URL inside an inline value (`tddy-daemon-sandbox`), and every OpenCode launch path's tests (`tddy-agent-launch`, `tddy-coder`, `tddy-integration-tests`), plus the deletion of the `opencode-route-file` adapter. Re-scored from 3 on 2026-11-07, when the user widened the scope to every OpenCode path.

## Initial Discovery

[2026-11-07-opencode-route-inline-from-spec-initial-discovery.md](./2026-11-07-opencode-route-inline-from-spec-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-opencode-acp-route-file-has-no-session-owner.md](../todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md) | ✅ RESOLVED HERE, closed as **"file not needed"** (user, 2026-11-07); delete at wrap |

No `Claimed by:` record exists in the touched packages.

## Affected Packages

- **tddy-agent-backend**:
  - `provider_route/support.rs`: `RouteShape::InlineConfig` (YAML `inline_config`); `bodies_accept(InlineConfig)` = OpenAI Responses, OpenAI Chat Completions; `RoutedAgent.config_env` (the spec's `route.config_env`).
  - `provider_route/by_spec.rs`: `inline_config_route`, dispatched by `resolve_route` for `(InlineConfig, _)`. `body_has_its_inputs` needs `key_env` **and** `config_env`.
  - `adapters/opencode_route_file.rs`, and its registration in `adapters/acp_hooks.rs` and `adapters/mod.rs`: **deleted** (user-approved 2026-11-07, "All paths").
- **tddy-coding-agent-specs**:
  - `routed_agent` copies `route.config_env` into `RoutedAgent.config_env`.
  - `route_slot_errors`, called from `parse_section`: refuses an `inline_config` slot without `config_env`, with a `TDDY_*` one, or with a `files_dir`.
  - `specs/opencode.yaml`: `route.shape: inline_config`, `config_env: OPENCODE_CONFIG_CONTENT`, no `files_dir`, no `acp.hooks_adapter`. Comments rewritten.
- **tddy-session-lifecycle**: `coding_agent_launcher/acp.rs` loses the `shape == JsonConfigFile` branch, `opencode_route_config` and the `OPENCODE_CONFIG_CONTENT` import. It refuses a route that carries files, naming the shape. Doc comments updated (`acp.rs:33-36`, :246-250; `coding_agent_launcher.rs:13`, :386-390).
- **tddy-agent-launch**: no logic change expected. `cli_route_launch` writes `route.files` (there are none) and applies `pty_route_env`. `jail_route_config_env` needs a file. `build_runner_plan` (`tddy-sandbox-recipes/src/plan.rs:197-204`) already joins `route.env` to the jail's env.
- **tddy-coder**: no logic change expected. `SpecAcpBackend::driver` applies `route_env` and, with no adapter named, prepares nothing.
- **tddy-daemon-sandbox**: `tool_jail::route_hosts` (:179-188) reads the `http(s)` URLs **inside** each route variable's value, not only a value that is itself a URL. An inline configuration names its base URL inside a JSON value. Without this a jailed `opencode-cli` on an account would have no egress to its endpoint. Found in pass 2; the jail's egress test (`each_jail_reaches_only_its_declared_hosts_plus_its_routes_base_url_host`) pins it end to end.
- **tddy-integration-tests**: `opencode_acp_backend.rs` builds its backend with no hooks adapter and pins the inline route (guard).
- **tddy-coding-subagent**: `OPENCODE_CONFIG_CONTENT` (`confinement.rs:12`, `lib.rs:22`) loses its last production user; delete it if green leaves it unused.
- **Docs only**: `supervisor.yaml.production:115-116`, `docs/ft/supervisor/tddy-supervisor.md:92`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-opencode-route-inline-from-spec.md](../../ft/daemon/1-WIP/PRD-2026-11-07-opencode-route-inline-from-spec.md)

## Summary

A new route shape, `inline_config`, puts the route's JSON configuration inline. It goes in the
variable the spec's `route.config_env` names. The **route body** renders it into the route's plain
environment, and every launcher already applies that. So no launcher names an agent or a variable.

OpenCode declares the shape. Every OpenCode launch carries the route inline: the daemon's ACP
subagent, `tddy-coder --agent opencode`, and `opencode-cli` on the PTY, jailed or not. No route file
is written.

## Scope

- [ ] `RouteShape::InlineConfig` and `RoutedAgent.config_env`
- [ ] `inline_config_route`; `body_has_its_inputs` needs `config_env`
- [ ] `route_slot_errors` load validation
- [ ] `opencode.yaml` at State B
- [ ] The daemon ACP planner has no shape- or agent-specific route branch, and refuses a route carrying files
- [ ] `opencode-route-file` adapter deleted
- [ ] `toml_args` and `env` routes unchanged (guards)
- [ ] `supervisor.yaml.production:115-116` and `tddy-supervisor.md:92` corrected (still stale at #836's tip `7dba6fdde`)
- [ ] Feature docs: `opencode-cli-session.md:172`, `opencode-acp-backend.md` § Running on a provider account, `provider-accounts.md`, `session-agent-roster.md:264`, `tddy-coding-agent-specs/docs/coding-agent-specs.md`, `tddy-agent-backend/docs/architecture.md`
- [ ] Wrap: delete `docs/dev/todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md`, recording "file not needed" and its three reasons in the change-history entry

## Technical Changes

### State A

Re-read at #836's tip `7dba6fdde`, which sits on #835's green `cac691ad7`.

- **Already done by #835.** The `harness == OPENCODE` branch is gone and `OPENCODE` is deleted.
  - `acp.rs:63-70` now reads `if shape == RouteShape::JsonConfigFile { merge_inline_config(&mut command, OPENCODE_CONFIG_CONTENT, …) }`.
  - It is keyed on the shape, but the variable is still hard-coded, and the shape still says "file".
- **Already done by #835.** `launch_agent` (`coding_agent_launcher.rs:383-399`) prepares the declared
  adapter through `acp_hooks_from_spec` (`acp.rs:252-289`) and hands it `route: None` ("D1 over D4").
  - So the daemon path already writes **no** route file.
  - The pass-1 test `a_daemon_acp_launch_writes_no_route_file_and_sets_no_opencode_config` would have
    been a guard, not red.
- **Superseded premise.** Pass 1 planned an `AcpSection.route_inline_env` field. The user replaced it
  with a route shape on 2026-11-07.
- **Still true on the `tddy-coder` ACP path** (`tddy-coder/src/spec_backend_build.rs:69-75`,
  `tddy-agent-backend/src/backend/spec_backend.rs:253-271`). The route goes to
  `opencode-route-file`, which writes `opencode.json` and sets `OPENCODE_CONFIG`.
- **Still true on the PTY path** (`tddy-agent-launch/src/cli_route_launch.rs:80-108`). The route's
  files are written into the session dir, and `route.config_env` (`OPENCODE_CONFIG`) names the first.
  The jail does the same under the scratch's `files_dir` (`cli_harness_jail.rs:360-377`,
  `tddy-sandbox-recipes/src/plan.rs:197-204`).
- **Still stale:** `supervisor.yaml.production:115-116` and `tddy-supervisor.md:92`.

### State B

- `route.shape: inline_config` serves `openai_responses` and `openai_chat` accounts. Its body is:
  - `env = [(config_env, <generated config JSON>)]` — the JSON carries no key, only `{env:<key_env>}`;
  - `secret_env = [(key_env, key)]`;
  - `files = []`;
  - `args = ["--model", "tddy-route/<model>"]`.
- Every launcher already applies `route.env`, so every path renders the shape with no code of its
  own:
  - daemon ACP (`harness_route_env`);
  - `tddy-coder` ACP (`acp_driver::route_env`);
  - PTY (`pty_route_env`);
  - jail (`build_runner_plan`).
- No file is written, and nothing sets `OPENCODE_CONFIG`.
- The daemon planner's only shape-specific route code is `toml_args`. A managed launch's
  `config_env` withdrawal merges over the route's value, which is already in `command.env`.
- A route that carries files is refused on the daemon ACP path, naming the shape.
- `json_config_file` stays a valid shape for PTY and jail launches. No built-in spec declares it.

## Testing Plan

- Every route an acceptance test uses is resolved **from the spec** (`resolve_route(&routed_agent(spec), …)`
  or `start_cli_route`), as production does. None is hand-built.
- The launcher unit tests in `coding_agent_launcher.rs` change their shared `a_route()` fixture. It
  now carries OpenCode's config in `env` rather than `files`, which is what a route body produces
  today. Without that change, the codex tests sharing the fixture would be refused once a file route
  is.

## Acceptance Tests

| Test | File | Kind |
|---|---|---|
| `a_routed_daemon_opencode_acp_launch_carries_its_route_inline_and_no_route_file` | `packages/tddy-session-lifecycle/tests/acp_agents_from_spec_acceptance.rs` | 🔴 |
| `a_drop_in_acp_agent_whose_spec_names_another_variable_gets_its_route_inline_there` | same | 🔴 |
| `a_managed_launchs_withdrawal_still_merges_over_the_inline_route` | same | 🔴 |
| `an_acp_launch_whose_route_carries_a_file_is_refused_naming_the_shape` | same | 🔴 (decision 1) |
| `a_route_still_reaches_an_acp_agent_as_overrides_only_when_its_spec_declares_toml_args` and `a_codex_acp_route_still_reaches_it_as_one_model_override_under_its_built_in_spec` (existing) | same | guard (`toml_args`) |
| `the_daemon_acp_launcher_names_no_agent_and_no_agent_variable` | `packages/tddy-session-lifecycle/tests/launcher_names_no_agent.rs` | 🔴 |
| `tddy_coder_agent_opencode_on_a_route_carries_it_inline_and_writes_no_route_file` | `packages/tddy-coder/tests/acp_backends_from_spec_acceptance.rs` | 🔴 |
| `an_opencode_cli_start_on_an_account_carries_its_route_inline_and_writes_no_file` | `packages/tddy-agent-launch/src/session_provider_route_tests.rs` | 🔴 (replaces the file pin) |
| `an_opencode_route_carries_the_config_inline_the_key_and_the_model_argument` | `packages/tddy-agent-backend/tests/opencode_route_acceptance.rs` | 🔴 (replaces the file pin) |
| `an_inline_config_route_without_config_env_fails_loading_naming_the_spec` | `packages/tddy-coding-agent-specs/tests/route_inline_config_load.rs` | 🔴 |
| `an_inline_config_route_naming_a_tddy_variable_fails_loading_naming_the_spec` | same | 🔴 |
| `an_inline_config_route_with_a_files_dir_fails_loading_naming_the_spec` | same | 🔴 |
| `opencode_declares_its_route_inline_in_opencode_config_content` | same | guard (contract YAML) |
| `a_json_config_file_route_still_loads_with_its_variable_and_directory` | same | guard |
| `every_built_in_specs_route_providers_are_ones_its_route_body_accepts_and_resolves` (existing) | `packages/tddy-coding-agent-specs/tests/route_body_drift_acceptance.rs` | 🔴 (OpenCode's body is the TODO) |
| `a_harness_profile_naming_opencode_starts_from_the_spec`, `a_routed_opencode_start_carries_the_same_route_config_inline_with_its_key_in_opencode_provider_key`, `jailed::host_and_jailed_opencode_cli_starts_and_resumes_spawn_stubs_whose_argv_env_and_inline_route_match_the_spec`, `jailed::each_jail_reaches_only_its_declared_hosts_plus_its_routes_base_url_host` (re-pinned) | `packages/tddy-session-lifecycle/tests/harness_clis_from_spec_acceptance.rs` | 🔴 |
| `a_sandboxed_opencode_session_runs_in_its_jail_on_the_provider_account` (re-pinned) | `packages/tddy-session-lifecycle/tests/sandboxed_cli_harness_acceptance.rs` | 🔴 |
| `a_routed_opencode_agent_gets_the_generated_config_inline_and_its_key` (re-pinned) | `packages/tddy-integration-tests/tests/opencode_acp_backend.rs` | guard |

## Unit Tests

| Test | File | Kind |
|---|---|---|
| `an_inline_config_route_lands_in_whatever_variable_the_spec_names` | `packages/tddy-agent-backend/tests/route_by_spec_unit.rs` | 🔴 |
| `an_inline_config_route_carries_no_key_in_its_plain_environment` | same | 🔴 |
| `an_inline_config_agent_without_a_config_variable_is_not_yet_routable` | same | 🔴 |
| `a_shapes_bodies_accept_the_providers_a_body_exists_for::case_4_inline_config` | same | guard (contract) |
| the existing `toml_args` / `env` body tests (`a_toml_args_route_declares_…`, `an_anthropic_env_route_pins_…`, `an_env_route_to_meta_…`) | same | guard |
| `a_base_url_an_inline_config_variable_carries_is_admitted_too` | `packages/tddy-daemon-sandbox/src/tool_jail.rs` | 🔴 |
| `a_resumed_opencode_session_regenerates_its_inline_config_from_the_vault_as_it_stands`, `an_opencode_start_records_its_account_and_model_but_never_its_key` (existing) | `packages/tddy-agent-launch/src/session_provider_route_tests.rs` | 🔴 (route resolution is the TODO) |
| `an_inline_routed_opencode_session_writes_no_file_and_sets_no_opencode_config` | `packages/tddy-agent-launch/src/cli_route_launch_tests.rs` | guard (the PTY applies route env) |
| `a_file_routed_session_on_fireworks_gets_a_config_file_naming_that_endpoint` (renamed; was the built-in OpenCode pin) | same | guard (`json_config_file`) |
| `a_provider_route_reaches_opencode_as_inline_config`, `a_managed_opencode_keeps_its_route`, `an_inline_opencode_config_that_is_not_json_is_refused_naming_it` (fixture re-keyed) | `packages/tddy-session-lifecycle/src/coding_agent_launcher.rs` | guard |

## Responsibility

How a provider route reaches an agent that takes inline JSON configuration, on **every** launch path:
- the `inline_config` route shape, its route body and its load validation;
- OpenCode's spec declaring it;
- launchers with no shape-, variable- or agent-specific route code;
- the deletion of the `opencode-route-file` adapter.

## Boundaries

- Does **not** change `codex-acp`'s route `-c` arguments or `route.shape: toml_args` (#835), nor the
  `env` shape.
- Does **not** remove `route.shape: json_config_file` (decision 2).
- Does **not** touch the supervisor's env handling (#836).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/acp-subagent-leftovers` (#835, `#agent-tools` 27) | `launch_agent` → `LaunchedAgent`, with declared hooks adapters prepared and handed `route: None`; `OPENCODE` deleted; `toml_args` keyed on `route.shape` | removes the remaining `JsonConfigFile` branch and its hard-coded variable | change hook preparation or the `toml_args` arm |
| `feature/agent-tools/env-allow-list` (#836, `#agent-tools` 28) | the agent env built from base + `pass_env` + launch; a relayed agent's env is not gated by `allowed_env_keys` | the inline variable is launch-set (route env), so it reaches the agent without a `pass_env` entry | change `agent_env` or the supervisor's relayed env |

#835 is green (`cac691ad7`), so no test here reaches a parent `TODO`. #836 is at its contract
(`7dba6fdde`). Its `TODO(env-allow-list)` bodies (`validate_env_entries`, `agent_env`) are on no path
these tests assert.

## Draft PR contract

PR: [#857](https://github.com/uppin/tddy-coder/pull/857) (`#agent-tools` 29/32).


- `RouteShape::InlineConfig` and its YAML word; `bodies_accept(InlineConfig)`.
- `RoutedAgent.config_env`, and `routed_agent` filling it.
- `inline_config_route` (body `TODO(opencode-route-inline)`), dispatched by `resolve_route`. The
  `body_has_its_inputs` arm is marked `TODO(opencode-route-inline)`.
- `route_slot_errors` (body `TODO(opencode-route-inline)`), called at load.
- `opencode.yaml` at State B.
- `TODO(opencode-route-inline)` markers on the daemon's `JsonConfigFile` branch, on `route_hosts` and on the adapter.
- The failing tests above.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** yes. #835 is green, and nothing here reaches #836's TODOs.
**Concurrent with:** `feature/agent-tools/user-mapping-in-supervisor` (`#agent-tools` 31)
**Blocks:** nothing

## Decisions & Trade-offs

- **Inline, by a generic route shape in YAML (user, 2026-11-07).** The spec declares `route.shape:
  inline_config`, and `route.config_env` names the variable. The file is not needed:
  - no secret is in it;
  - inline config outranks the worktree's `opencode.json`, while `OPENCODE_CONFIG` ranks below it, so
    a repository could redirect `baseURL` and harvest the routed key;
  - no directory owned by the session user exists.
- **All paths (user, 2026-11-07).** The daemon ACP, `tddy-coder` ACP and PTY `opencode-cli` paths all
  carry the route inline. The `opencode-route-file` adapter is deleted.
- **The route body renders the shape, not the launchers.** Every launcher already applies
  `route.env`, so putting the inline config there keeps every launch site generic. The alternative —
  a renderer each launcher calls with the spec's slot — would put the shape in four places.
- **`config_env` is reused, not a new field.** It already names "the variable the route's
  configuration reaches the agent in". For `json_config_file` it holds the file's path; for
  `inline_config` it holds the configuration itself.
- **`RoutedAgent` grows `config_env`.** A route body needs the variable. About 30 struct literals,
  mostly in tests, gain `config_env: None`.
- **Load validation.** An `inline_config` slot without `config_env`, with a `TDDY_*` one, or with a
  `files_dir` is a spec error, not a silent no-op.

- **ACP files-route refusal (user, 2026-11-07).** An ACP launch whose route carries files is refused,
  naming the shape. The daemon test pins it; no `tddy-coder` test is required.
- **`route.shape: json_config_file` stays (user, 2026-11-07)** for a drop-in PTY or jailed agent.
- **Deletion consent (user, 2026-11-07).** Green may delete these, **only if nothing uses them after
  green**: `tddy-coding-subagent`'s `OPENCODE_CONFIG_CONTENT` constant (`confinement.rs:12`,
  re-exported at `lib.rs:22`), and the test-support helpers `route_config_mode` and
  `route_config_text` (`tddy-session-lifecycle/tests/harness_clis_support/mod.rs`). The
  `opencode-route-file` adapter's deletion was already approved under "All paths".
- **The Codex name stays in the launcher (user, 2026-11-07).** `acp.rs` still calls
  `tddy_core::codex_acp_route_args` for `toml_args`. The scan test checks only "opencode".
  TODO(sweep): a todo note for the sweep node is to key the `toml_args` renderer off the shape, not
  a Codex-named helper, and widen the scan.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
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

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code
