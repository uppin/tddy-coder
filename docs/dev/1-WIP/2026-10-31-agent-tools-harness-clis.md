# Changeset: OpenCode, Muse Code and Warp CLI sessions launch from their tool definitions

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — `tddy-coding-agent-specs`, `tddy-agent-backend` (two adapters, `CliHarness` removed), `tddy-agent-launch`, `tddy-discovery` (harness profiles); provider-route files in the jail
**Stack:** `#agent-tools 8/12` — branch `feature/agent-tools/harness-clis`, base `feature/agent-tools/cursor-cli`
**PR:** [#797](https://github.com/uppin/tddy-coder/pull/797)
**PRD:** [PRD-2026-10-31-harness-clis-from-specs.md](../../ft/daemon/1-WIP/PRD-2026-10-31-harness-clis-from-specs.md)

## Initial Discovery

[2026-10-31-agent-tools-harness-clis-initial-discovery.md](./2026-10-31-agent-tools-harness-clis-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-sandboxed-cli-harness-session-start-sandboxed-cli-harness-session.md` | ✅ **RESOLVED HERE.** The function is replaced by `launch_jailed`. Claimed at commit 2 |
| [2026-10-24-warp-cli-sandbox.md](../todo/2026-10-24-warp-cli-sandbox.md) | ⚠ During — **narrowed at wrap** (decided 2026-10-31: Warp stays refused). The refusal moves to `warp.yaml`'s `placements: [host]`, and the entry's "what would close it" is rewritten as spec terms: a `files`/`reads` entry for Warp's config dir, the `warp-resume-token` adapter on the jailed launch, and `placements: [host, jail]` |
| [2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md](../todo/2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md) | — Unrelated here (#799) |

## Affected Packages

- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `opencode.yaml`, `muse.yaml`, `warp.yaml`; `route`, `models.list_command`, `resume` and `placements` fields
- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): adapters `opencode-models` and `warp-resume-token`; the `CliHarness` table and its descriptors removed
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): opencode, muse and warp starts and resumes on the shared launchers; session-type → spec-id resolution
- [ ] [`tddy-discovery`](../../../packages/tddy-discovery/README.md): harness profiles resolve `harness:` to a spec id
- [ ] [`tddy-sandbox-recipes`](../../../packages/tddy-sandbox-recipes/README.md): route files and config env come from the spec's `route` section

## Responsibility

- `CodingAgentSpec.{route: Option<RouteSlot { required_in_jail, config_env, files_dir }>, models.list_command, models.adapter, resume: Option<ResumeSlot { adapter }>, placements: Vec<Placement>}`.
- Three specs:
  - `opencode.yaml`: `--continue`, `--model`/`--prompt`, `OPENCODE_CONFIG`;
  - `muse.yaml`: no-JIT policy, `META_API_KEY`;
  - `warp.yaml`: no model, `placements: [host]`.
- Adapters `opencode-models` (the `opencode models` output) and `warp-resume-token` (scrapes the PTY exit output).
- `tddy_coding_agent_specs::spec_for_session_type(&str) -> Option<&str>`.
- `CLI_HARNESSES`, `cli_harness_muse.rs` and `cli_harness_warp.rs` are deleted. `svc_start_sandboxed_cli_harness_session` becomes a `launch_jailed` caller or is deleted.
- A sandboxed start whose spec placements exclude `jail` is refused `FAILED_PRECONDITION`, naming the placements.

## Boundaries

- No OpenCode ACP (#800). No standalone app (#799).
- `SandboxRecipe::{OpenCodeCli, MuseCli}` stay for the app until #799.
- daemon.yaml `cli_harnesses.<type>.binary_path` stays as an override until #801.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` claude-host | `launch_host_pty`, slots, `CodingAgentSpec` | host starts | change existing slot kinds |
| `n5` claude-jail | `launch_jailed`, generic sections, `--exec-plan` | jailed opencode and muse starts | change the launcher |
| `n3` egress-allowlist | `network` and enforcement | declared hosts plus the route base URL | change `admits` |
| `n6` claude-headless | `AdapterRegistry` | registers two PTY-side adapters | change the registry trait |
| `n7` cursor-cli | nothing consumed | sits above #796 because of the line | touch `cursor.yaml` |

## Draft PR contract

The first push publishes the new spec fields, the three specs, the two adapters' signatures, `spec_for_session_type` and the failing tests below. Unimplemented bodies are `// TODO(harness-clis): implement`.

## Green wave

**Wave:** 4 of 7
**Greenable independently:** after #794 (`launch_jailed`), #792 (egress) and #795 (adapter registry) are green
**Concurrent with:** #796 cursor-cli
**Blocks:** #799 jail-agnostic (the app uses these specs), #800 acp-agents (`opencode.yaml`'s ACP variant)

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- `cli_harness.rs`: the `CliHarness` fn-pointer descriptors (session type, label, route harness, context agent, default binary, binary env, argv fn, models, `route_config_env`).
- `svc_start_sandboxed_cli_harness_session` (over budget) and `cli_harness_jail.rs::sandboxed_agent_kind`, which refuses Warp.
- The `SandboxRecipe::{OpenCodeCli, MuseCli}` arms; `route_files.rs`.

## State B

- The three agents launch from specs on both launchers.
- Route files and config env come from the spec. Warp's jail refusal is spec-driven.
- `CliHarness` is gone.

## Delta

### New
- `packages/tddy-coding-agent-specs/specs/{opencode,muse,warp}.yaml`
- `packages/tddy-agent-backend/src/adapters/{opencode_models.rs, warp_resume_token.rs}`
### Modified
- `tddy-agent-launch` `opencode_cli_spawn.rs`, `cli_harness_jail.rs`, `cli_resume_token.rs`, `harness_profile_start.rs`.
- `tddy-discovery` `harness_profile.rs`.
- Docs at wrap: `docs/ft/daemon/{opencode,muse,warp}-cli-session.md`, `harness-profiles.md`.
### Removed
- `packages/tddy-agent-backend/src/{cli_harness.rs, cli_harness_muse.rs, cli_harness_warp.rs}` (their data moves into specs)

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/harness_clis_from_spec_acceptance.rs`**

- [x] `host_and_jailed_opencode_cli_starts_and_resumes_spawn_stubs_whose_argv_env_and_route_files_match_the_spec` (macOS `jailed` module; its host half is `host::a_host_opencode_start_and_resume_render_opencode_yamls_argv`, see `### From /red`)
- [x] `host_and_jailed_muse_cli_starts_and_resumes_do_the_same` (macOS `jailed`; host half `host::a_host_muse_start_and_resume_render_muse_yamls_argv`)
- [x] `a_jailed_opencode_start_without_a_route_is_refused_as_today`
- [x] `a_warp_cli_host_session_records_its_resume_token_through_the_adapter`
- [x] `a_sandboxed_warp_cli_start_is_refused_failed_precondition_naming_the_placements`
- [x] `each_jail_reaches_only_its_declared_hosts_plus_its_routes_base_url_host` (macOS `jailed`)
- [x] `a_harness_profile_naming_opencode_starts_from_the_spec`

Fixture: `packages/tddy-session-lifecycle/tests/harness_clis_support/mod.rs`.

### Unit / integration tests

- [x] `tddy-agent-backend` adapters: `opencode-models` parses recorded `opencode models` output; `warp-resume-token` extracts the token from recorded exit output, and returns `None` when there is none. Plus `LaunchAdapters::builtin` lists and resolves both.
- [x] `tddy-coding-agent-specs`: `spec_for_session_type` for every session type, and an unknown one is `None`. Plus `spec::placement_tests` (`require_placement`), `slots::hooks` (`UnsupportedHooks` refuses hook params) and `tests/harness_specs_acceptance.rs` (the three specs' new sections through the built-in registry).
- [x] `tddy-agent-launch`: `harness_from_spec::tests` (`harness_template_params`, `require_spec_placement`) and `harness_profile_start_tests` (`profile_spec_id`).

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Launch | Integration with stubs | Behaviour-preserving |
| Adapters | Unit over recordings | Deterministic |

## Decisions & Trade-offs

- **`placements` is spec data**, which makes "Warp cannot be jailed" a reviewable line in `warp.yaml`.
- **The `CliHarness` descriptors are deleted, not wrapped.** Every field has a spec home.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-harness-clis-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-10-31: pass 2 runs without per-node review
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

## Refactoring Needed
### From /red (TDD red phase)

**Red baseline (2026-10-31).** Scoped runs: `tddy-coding-agent-specs` (all targets), `tddy-agent-backend --lib`, `tddy-agent-launch --lib`, and `tddy-session-lifecycle` (`harness_clis_from_spec_acceptance`, `opencode_cli_session`, `warp_cli_session`). Clippy `-D warnings` clean on the first three (all targets) and on the new lifecycle test target; `cargo fmt --all --check` clean.

- **Fails for this node's reason** (a `todo!("harness-clis: …")`):
  - specs `spec::placement_tests`, 3 tests: `CodingAgentSpec::require_placement`.
  - specs `spec_for_session_type_tests`, 2 tests: `spec_for_session_type`.
  - specs `slots::hooks::tests::the_unsupported_hooks_adapter_refuses_hook_params`: `UnsupportedHooks::try_render`.
  - backend `adapters::launch::tests`, 2 tests: `LaunchAdapters::builtin`.
  - backend `adapters::opencode_models::tests`, 2 tests: `OpenCodeModels::parse_models`.
  - backend `adapters::warp_resume_token::tests`, 2 tests: `WarpResumeToken::resume_token`.
  - launch `harness_from_spec::tests`, 6 tests: `harness_template_params` (4) and `require_spec_placement` (2).
  - launch `harness_profile_start_tests`, 2 tests: `profile_spec_id`.
- **Sequencing facts** (a parent's `todo!`):
  - All 9 `harness_clis_from_spec_acceptance` tests panic in `claude-host: load_agent_registry` (#791): the fixture loads the agent registry as the daemon runtime does.
  - All 4 `harness_specs_acceptance` tests panic in the same `todo!` (#791).
  - Behind those: `launch_host_pty` and `HostCodingAgents::resolve` (#791), `launch_jailed` (#794), `Egress::deserialize`/`admits` for the egress test (#792), and `slots::render` (#791, with #796's Cursor arms).
- **Pre-existing, not touched:** the parents' own `todo!` failures in the touched packages — specs `slots::tests`, `slots::cursor::tests`, `headless::tests`, `spec_load_acceptance`, `headless_render_golden`, `cursor_spec_acceptance`; backend `adapters::tests` and `adapters::claude_stream_json::tests` (`claude-headless`); launch `cursor_from_spec::tests` (`cursor-cli`).
- **Still green:** `opencode_cli_session` (23), `warp_cli_session` (14), and the rest of the backend (116) and launch (171) libs. No production path routes through a `harness-clis` `todo!`: today's `CliHarness` launches are unchanged, with `TODO(harness-clis)` markers where green switches them (`cli_harness_jail.rs`, `svc_start_sandboxed_cli_harness_session.rs`, `opencode_cli_spawn.rs`, `cli_resume_token.rs`, `cli_route_launch.rs`, `harness_profile_start.rs`).

**Deviations from the planned surface, and why:**

- The new fields live on `CodingAgentSection` (the YAML's `extensions.coding_agent`), read as `spec.agent.{route, resume, placements, models.list_command, models.adapter}`, not on `CodingAgentSpec` itself. That struct is #791's `{id, run, agent}`; a field there would have no YAML home.
- `RouteSlot` gained `key_env` beyond the planned `{required_in_jail, config_env, files_dir}`. `MUSE_ROUTE_KEY_ENV` (`META_API_KEY`) and `OPENCODE_ROUTE_KEY_ENV` live in the backend files green deletes, so the spec must carry them.
- `models.list_command` is a `Vec<ArgvItem>` (`["{binary}", models]`), rendered with the run grammar, rather than a bare arg list.
- The two adapters are not `StreamAdapter`s. A model listing and a terminal scrape share nothing with a stream, so they are new traits `ModelsAdapter` / `ResumeTokenAdapter` and a separate registry `LaunchAdapters` in `tddy_agent_backend::adapters::launch`. #795's `AdapterRegistry` is unchanged.
- Additive parent-type changes:
  - `McpSlot::Unsupported` (`mcp: { kind: unsupported }`). The three agents take no MCP registration on today's launch, and #791's `mcp` slot is required.
  - `SlotError::HooksUnsupported`.
  - A hooks adapter named `unsupported` (`UnsupportedHooks`), because #791's `hooks: HooksSlot { adapter }` is required and the three agents have no hooks. **Developer decision:** the faithful form is `hooks: unsupported`, a `HooksSlot | Unsupported` amendment to #791 like the `ToolSlot` one. The adapter is a stand-in until then, marked `TODO(harness-clis)`.
  - `TODO(harness-clis)` comment lines inside #791's `todo!` bodies (`CodingAgentExtension::validate`, `hooks::adapter`, `slots::render`), and the new `ModelsSection`/`CodingAgentSection` fields in #791's `slots::tests` literal.
- Harness-profile resolution lives in `tddy-agent-launch` (`harness_profile_start::profile_spec_id`), not in `tddy-discovery`. Discovery does not depend on the specs crate, and `spec_for_session_type` already does the resolving; `tddy-discovery` is untouched.
- The warp refusal is `harness_from_spec::require_spec_placement` → `FAILED_PRECONDITION` reading `the sandbox runner does not host warp-cli: warp cannot run in the jail: its spec's placements are [host]`. That message keeps today's prefix, so `warp_cli_session::a_sandboxed_warp_session_is_refused` stays green across the switch.
- The two `host_and_jailed_*` tests were split in two:
  - their host halves run on every OS (`host::a_host_{opencode,muse}_start_and_resume_render_*_yamls_argv`);
  - the jailed halves keep the planned names and sit in the macOS-only `jailed` module, as #796's do.
- The egress test runs OpenCode on an OpenAI-compatible account whose base URL is a local stand-in, because Meta's base URL is fixed by its preset. Muse on Meta asserts it reaches neither stand-in.
- `opencode.yaml`/`muse.yaml` carry a commented `network:` with `hosts: []` (TODO), as #794/#796 did. `warp.yaml` has no jail sections: `placements: [host]`.

**`TODO(...)` deletions needing approval (CLAUDE.md § ASK):** green deletes `packages/tddy-agent-backend/src/{cli_harness.rs, cli_harness_muse.rs, cli_harness_warp.rs}`, and deletes `svc_start_sandboxed_cli_harness_session.rs` or turns it into a `launch_jailed` caller. Each is marked in the source.

**Code-issue claim:** `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-sandboxed-cli-harness-session-start-sandboxed-cli-harness-session.md` — `**Claimed by:** #797`, `**Lands after:** #796`.
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
