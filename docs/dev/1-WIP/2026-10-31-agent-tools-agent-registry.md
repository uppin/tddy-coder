# Changeset: One coding-agent registry: agent enums and string gates become spec capabilities

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature (breaking configuration change)
**Complexity**: 5 — six or more crates (`tddy-agent-backend`, `tddy-coder`, `tddy-session-activity`, `tddy-session-lifecycle`, `tddy-daemon-rpc`, `tddy-daemon-kernel`, `tddy-worktree-service`, `tddy-web` picker); breaking config removal; ~15 string gates
**Stack:** `#agent-tools 12/12` — branch `feature/agent-tools/agent-registry`, base `feature/agent-tools/acp-agents`
**PRD:** [PRD-2026-10-31-coding-agent-registry.md](../../ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md)

## Initial Discovery

[2026-10-31-agent-tools-agent-registry-initial-discovery.md](./2026-10-31-agent-tools-agent-registry-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-10-22-tool-session-provider-picker-needs-harness-metadata.md](../todo/2026-10-22-tool-session-provider-picker-needs-harness-metadata.md) | ✅ **RESOLVED HERE.** `AgentInfo` carries the spec's kind, harness and route support, and the pane asks for routes only for agents whose spec supports them |
| `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` | ⚠ During. Seams A–D are the model, selection and context tables that move into the registry. Re-measure at wrap, and promote to ✅ if closed |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md` | ⚠ During. The session-type `match` in `start_here` becomes a spec lookup |
| [2026-09-16-the-shipped-binary-set-is-restated-in-eight-places-and-one-is-pinned.md](../todo/2026-09-16-the-shipped-binary-set-is-restated-in-eight-places-and-one-is-pinned.md) | — Unrelated (no new binary) |

## Affected Packages

- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `capabilities`, `route.providers`/`shape`, `answer_wait_secs`; `CodingAgentRegistry` (list, by id, by session type)
- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): `AnyBackend`, `Harness`, `build_backend` and the selection, model and context tables replaced by registry lookups
- [ ] [`tddy-coder`](../../../packages/tddy-coder/README.md): `create_backend`, `BUILTIN_BACKEND_AGENT_IDS`, `--agent` and the interactive selection read the registry
- [ ] [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md): hook reports, question answers and operator notifications gate on capabilities
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): session deletion, project-home and split gate on capabilities
- [ ] [`tddy-daemon-rpc`](../../../packages/tddy-daemon-rpc/README.md): notify gate; `AgentInfo` gains kind, harness and route support
- [ ] [`tddy-worktree-service`](../../../packages/tddy-worktree-service/README.md): branch intent gates on a capability
- [ ] [`tddy-daemon-kernel`](../../../packages/tddy-daemon-kernel/README.md): daemon.yaml `claude_cli:`, `cursor_cli:` and `cli_harnesses:` removed; start refuses them with the replacement named
- [ ] [`tddy-web`](../../../packages/tddy-web/README.md): the provider picker reads route support

## Responsibility

- `CodingAgentSpec.capabilities: BTreeSet<Capability>`, with `Capability::{Hooks, AskUserQuestion, SplitHost, ProjectHomeMain, RequiresModel, BranchNameRequired, KeepWorktreeOnDelete, OperatorNotify}`.
- `CodingAgentSpec.route: RouteSupport { providers: Vec<ProviderKind>, shape: RouteShape::{Env, Args, ConfigFile} }`, which replaces `provider_route::Harness`'s matrix.
- `tddy_coding_agent_specs::CodingAgentRegistry::{all(), get(id), for_session_type(st)}`.
- Every `session_type == "claude-cli"` or `"cursor-cli"` gate is replaced by `spec.has(Capability::…)`. A source-scan test pins that no gate names an agent.
- `tddy_daemon_kernel` config refuses `claude_cli`, `cursor_cli` and `cli_harnesses` with `moved to <tddyhome>/tools/<id>.yaml (<field> → <spec field>)`.
- `AgentInfo.{kind, harness, route_providers}`, and the web pane asks for routes only where `route_providers` is non-empty.

## Boundaries

- No spec-editing UI. No new agent.
- Adapters (the Rust protocol code) stay where they are.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2`–`n11` | every agent spec, both launchers, the headless and ACP paths, adapters, tool-agnostic jails | the registry enumerates them; gates read their capabilities | change any launcher, slot or adapter contract |
| `n1` tool-defs (#790) | `<tddyhome>/tools` overrides | the replacement for the removed daemon.yaml keys | change the override rule |

## Draft PR contract

The first push publishes `Capability`, `RouteSupport`, `CodingAgentRegistry`, the `AgentInfo` proto fields, the config refusal error type, each spec's `capabilities` lines, the source-scan test, and the failing tests below. Unimplemented bodies are `// TODO(agent-registry): implement`.

## Green wave

**Wave:** 7 of 7
**Greenable independently:** after every earlier node is green: the gates and enums it collapses are only fully spec-backed then
**Concurrent with:** none
**Blocks:** nothing (top of the stack)

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- There are 10 or more parallel agent enumerations: `AnyBackend`, `Harness`, `HeadlessHarness` (removed in #9), `CLI_HARNESSES` (removed in #8), `SandboxRecipe` arms (removed in #10), `AgentKind` (removed in #10), `ACP_HARNESSES` (removed in #11), `BUILTIN_BACKEND_AGENT_IDS`, two backend-construction matches, and the selection, model and context string tables.
- Session-type string gates are in `svc_start_session_core.rs:382,421`, `hook_reports.rs:175`, `operator_notifications.rs:39`, `question_answers.rs:69`, `notify.rs:26`, `session_deletion.rs:47`, `branch_intent.rs`, `cli_harness_requires_model`, `harness_profile_start.rs` and `project_home_launch.rs`.
- daemon.yaml carries `claude_cli:` (binary, tools path, daemon url, home dir, fullscreen, answer wait), `cursor_cli:` and `cli_harnesses:`.

## State B

- The registry is the only enumeration, and every gate is a capability.
- Adding an agent is a YAML file.
- The removed daemon.yaml keys fail start-up with the replacement named.

## Delta

### New
- `packages/tddy-coding-agent-specs/src/{registry.rs, capability.rs, route.rs}`
- `packages/tddy-session-lifecycle/tests/no_agent_name_gates.rs` (source scan)
### Modified
- Every gate site listed in State A.
- `tddy-agent-backend` `backend/mod.rs`, `provider_route.rs`, `model_catalog.rs`, `context_globs.rs`; `tddy-coder` `run.rs`, `config.rs`; `tddy-daemon-kernel` `config/agent_cli.rs`.
- `tddy-service` proto `AgentInfo`; `tddy-web` `useCreateSessionModel.ts`.
- `config.example.yaml`, `dev.daemon.yaml`, `desktop.yaml.production`.
- Docs at wrap: `docs/ft/daemon/session-agent-roster.md`, `harness-profiles.md`, all `*-cli-session.md`; a changelog entry for the breaking change.
### Removed
- `AnyBackend` (replaced by a spec-plus-adapter backend), `provider_route::Harness`, `BUILTIN_BACKEND_AGENT_IDS`, and the daemon.yaml agent sections

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/yaml_only_agent_acceptance.rs`**

- [ ] `a_new_agent_dropped_into_tddyhome_tools_appears_in_the_hosts_agent_list_starts_on_the_host_and_in_a_jail_and_offers_its_declared_capabilities` (stub binary, existing adapters, no Rust change)
- [ ] `removing_hooks_from_an_override_of_claude_yaml_turns_off_hook_based_activity_for_claude`

**`tddy-daemon`: `packages/tddy-daemon/tests/removed_agent_config_acceptance.rs`**

- [ ] `a_daemon_yaml_with_claude_cli_fails_startup_naming_tddyhome_tools_claude_yaml`
- [ ] `the_same_for_cursor_cli_and_cli_harnesses`

**`tddy-coder`: `packages/tddy-coder/tests/agent_registry_cli_acceptance.rs`**

- [ ] `tddy_coder_agent_accepts_any_registry_id_and_the_interactive_selection_lists_the_registry`

**`tddy-web`: `packages/tddy-web/cypress/component/CreateSessionProviderPicker.cy.tsx`**

- [ ] `does not request provider routes for an agent whose route support is empty` (`mountWithRpc` + `anInMemoryRpcBackend`)

**`tddy-daemon-rpc`: `packages/tddy-daemon-rpc/tests/agent_catalog_acceptance.rs`**

- [ ] `the_agent_catalog_reports_kind_harness_and_route_support_per_agent`

### Unit / integration tests

- [ ] `tddy-coding-agent-specs` registry: `for_session_type` for every built-in; an unknown capability is refused at load; `RouteSupport` parsing.
- [ ] `tddy-daemon-kernel` config: each removed key yields its specific migration message.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| YAML-only agent | Integration with a stub binary | The stack's headline claim; only an end-to-end start proves it |
| Gates | Source scan plus targeted integration | A scan keeps the property; integration proves the capability path |
| Web picker | Cypress component | UI behaviour |

## Decisions & Trade-offs

- **Breaking removal, refused loudly.** The developer chose migration into specs over permanent dual config, so the daemon names the replacement rather than ignoring the key.
- **Capabilities are a closed Rust enum.** A capability is code that reads it, so an unknown capability in YAML is an error.
- Complexity 5. Consider greening by capability group: activity gates, lifecycle gates, then config removal.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-agent-registry-initial-discovery.md`)
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
