# Changeset: OpenCode, Muse Code and Warp CLI sessions launch from their tool definitions

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — `tddy-coding-agent-specs`, `tddy-agent-backend` (two adapters, `CliHarness` removed), `tddy-agent-launch`, `tddy-discovery` (harness profiles); provider-route files in the jail
**Stack:** `#agent-tools 8/12` — branch `feature/agent-tools/harness-clis`, base `feature/agent-tools/cursor-cli`
**PRD:** [PRD-2026-10-31-harness-clis-from-specs.md](../../ft/daemon/1-WIP/PRD-2026-10-31-harness-clis-from-specs.md)

## Initial Discovery

[2026-10-31-agent-tools-harness-clis-initial-discovery.md](./2026-10-31-agent-tools-harness-clis-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-sandboxed-cli-harness-session-start-sandboxed-cli-harness-session.md` | ✅ **RESOLVED HERE.** The function is replaced by `launch_jailed`. Claimed at commit 2 |
| [2026-10-24-warp-cli-sandbox.md](../todo/2026-10-24-warp-cli-sandbox.md) | ⚠ During — **narrowed at wrap** (decided 2026-10-31: Warp stays refused). The refusal moves to `warp.yaml`'s `placements: [host]`, and the entry's "what would close it" is rewritten as spec terms: a `files`/`reads` entry for Warp's config dir, the `warp-resume-token` adapter on the jailed launch, and `placements: [host, jail]` |
| [2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md](../todo/2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md) | — Unrelated here (#10) |

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

- No OpenCode ACP (#11). No standalone app (#10).
- `SandboxRecipe::{OpenCodeCli, MuseCli}` stay for the app until #10.
- daemon.yaml `cli_harnesses.<type>.binary_path` stays as an override until #12.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` claude-host | `launch_host_pty`, slots, `CodingAgentSpec` | host starts | change existing slot kinds |
| `n5` claude-jail | `launch_jailed`, generic sections, `--exec-plan` | jailed opencode and muse starts | change the launcher |
| `n3` egress-allowlist | `network` and enforcement | declared hosts plus the route base URL | change `admits` |
| `n6` claude-headless | `AdapterRegistry` | registers two PTY-side adapters | change the registry trait |
| `n7` cursor-cli | nothing consumed | sits above #7 because of the line | touch `cursor.yaml` |

## Draft PR contract

The first push publishes the new spec fields, the three specs, the two adapters' signatures, `spec_for_session_type` and the failing tests below. Unimplemented bodies are `// TODO(harness-clis): implement`.

## Green wave

**Wave:** 4 of 7
**Greenable independently:** after #5 (`launch_jailed`), #3 (egress) and #6 (adapter registry) are green
**Concurrent with:** #7 cursor-cli
**Blocks:** #10 jail-agnostic (the app uses these specs), #11 acp-agents (`opencode.yaml`'s ACP variant)

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

- [ ] `host_and_jailed_opencode_cli_starts_and_resumes_spawn_stubs_whose_argv_env_and_route_files_match_the_spec`
- [ ] `host_and_jailed_muse_cli_starts_and_resumes_do_the_same`
- [ ] `a_jailed_opencode_start_without_a_route_is_refused_as_today`
- [ ] `a_warp_cli_host_session_records_its_resume_token_through_the_adapter`
- [ ] `a_sandboxed_warp_cli_start_is_refused_failed_precondition_naming_the_placements`
- [ ] `each_jail_reaches_only_its_declared_hosts_plus_its_routes_base_url_host`
- [ ] `a_harness_profile_naming_opencode_starts_from_the_spec`

### Unit / integration tests

- [ ] `tddy-agent-backend` adapters: `opencode-models` parses recorded `opencode models` output; `warp-resume-token` extracts the token from recorded exit output, and returns `None` when there is none.
- [ ] `tddy-coding-agent-specs`: `spec_for_session_type` for every session type, and an unknown one is `None`.

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
