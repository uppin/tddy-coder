# Changeset: ACP coding agents launch from their tool definitions

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — `tddy-coding-agent-specs`, `tddy-agent-backend` (ACP backends, hooks adapters), `tddy-session-lifecycle` (`DaemonCodingAgentLauncher`), `tddy-coding-subagent` (confinement); tool deps drive binary resolution
**Stack:** `#agent-tools 11/12` — branch `feature/agent-tools/acp-agents`, base `feature/agent-tools/jail-agnostic`
**PR:** [#800](https://github.com/uppin/tddy-coder/pull/800)
**PRD:** [PRD-2026-10-31-acp-agents-from-specs.md](../../ft/coder/1-WIP/PRD-2026-10-31-acp-agents-from-specs.md)

## Initial Discovery

[2026-10-31-agent-tools-acp-agents-initial-discovery.md](./2026-10-31-agent-tools-acp-agents-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| `packages/tddy-coding-subagent/docs/code-issues/complexity-session-take-turn.md`, `oversized-file-session.md` | — Unrelated. The session's turn logic is not touched, only how the command is planned |

## Affected Packages

- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `claude-acp.yaml`, `codex-acp.yaml`, the `opencode.yaml` ACP variant; `transport: acp`; slot kinds `acp_session` (MCP), `acp_meta` and `config_env` (withdrawal)
- [ ] [`tddy-actions`](../../../packages/tddy-actions/README.md): `Transport::Acp`
- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): hooks adapters `codex-acp-login`, `opencode-route-file`; `AcpBackend`, `CodexAcpBackend`, `OpenCodeAcpBackend` plan from specs
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): `DaemonCodingAgentLauncher` ACP arms read specs; `ACP_HARNESSES` removed
- [ ] [`tddy-coding-subagent`](../../../packages/tddy-coding-subagent/README.md): `confinement_for` reads the spec's `tool_deny` kind

## Responsibility

- `Transport::Acp` and `CodingAgentSection.acp: Option<AcpSection { command: Vec<ArgvItem>, env, hooks_adapter: Option<String>, mcp, tool_deny, withdraw_native }>`.
- `McpSlot::AcpSession` and #791's `ToolSlot` gains `AcpMeta { path }` and `ConfigEnv { env }` (beside `Flag` and `Unsupported`).
- `ToolBinary.sibling_of` and `resolve_sibling`; the hooks-adapter registry `AcpHooksAdapters`.
- `claude-acp.yaml`: `bunx claude-agent-acp`, overridable by `TDDY_CLAUDE_AGENT_ACP_CLI`; `tool_deny: acp_meta claudeCode.options.disallowedTools`.
- `codex-acp.yaml`: `deps: [codex]`. The binary resolves from the dep's install. Route args are a template, `hooks_adapter: codex-acp-login`, `tool_deny: unsupported`.
- `opencode.yaml` gains an `acp` variant: `opencode acp`, `tool_deny: config_env OPENCODE_CONFIG_CONTENT`, the route config as a declared file.
- `confinement_for(spec)` derives `CannotWithdraw` from `tool_deny: Unsupported`.

## Boundaries

- The ACP driver protocol is unchanged.
- No new ACP agents.
- Routing semantics are unchanged (#801 moves route compatibility into spec data).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n9` headless-others | `codex.yaml`, `McpSlot::{Inline, Argv}` | `codex-acp` depends on `codex` | change `codex.yaml`'s headless section |
| `n8` harness-clis | `opencode.yaml`, `route` slot | adds the ACP variant | change the PTY section |
| `n6` claude-headless | `AdapterRegistry` | its pattern only: the hooks adapters have a registry of their own, `AcpHooksAdapters` (neither n6's stream registry nor n8's `LaunchAdapters` has the shape) | change the trait |
| `n2` claude-host (#791) | `load_agent_registry`, `ToolRegistry::from_embedded`/`load_with_owners`, `CodingAgentExtension::validate`, `HostCodingAgents::resolve` | every session-lifecycle acceptance test loads the registry through them | implement them |
| `n1` tool-defs (#790) | `deps` and `resolve_closure` | `codex-acp` resolves through its dep | change resolution |

Resolving a binary **from a dep's install** needs a resolution source on #790's `ToolBinary`: `sibling_of: <dep id>`. It is additive, and it is the only `tddy-actions` resolution change in this node.

## Draft PR contract

The first push publishes `Transport::Acp`, `AcpSection`, the new slot variants, `ToolBinary.sibling_of`, the two specs and the variant, the hooks-adapter registrations, and the failing tests below. Unimplemented bodies are `// TODO(acp-agents): implement`.

## Green wave

**Wave:** 6 of 7
**Greenable independently:** after #798 (`codex.yaml`) and #797 (`opencode.yaml`) are green — and after #790 (`resolve_closure`) and #791 (`load_agent_registry`, `HostCodingAgents::resolve`), which every acceptance test below runs through first
**Concurrent with:** none
**Blocks:** #801 agent-registry

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- `backend/acp.rs`, `codex_acp.rs` (resolved beside `codex`) and `opencode_acp.rs` hand-build commands.
- `coding_agent_launcher.rs:62,283,292,305-344,737` has per-agent arms.
- `confinement.rs` has `CLAUDE_NATIVE_TOOLS` and the per-harness withdrawal.
- `ACP_HARNESSES` enumerates the agents.

## State B

- ACP agents plan from specs: the command, the MCP servers as `session/new` `mcp_servers`, and withdrawal by the declared kind.
- Codex-acp resolves through its dep.

## Delta

### New
- `packages/tddy-coding-agent-specs/specs/{claude-acp,codex-acp}.yaml`
- `packages/tddy-agent-backend/src/adapters/{codex_acp_login.rs, opencode_route_file.rs}`
### Modified
- `backend/{acp,codex_acp,opencode_acp}.rs`; `coding_agent_launcher.rs`; `confinement.rs`.
- `tddy-actions` `tool/{def,resolve}.rs` (`sibling_of`).
- Docs at wrap: `docs/ft/coder/{acp-agent,codex-acp-backend,opencode-acp-backend}.md`.

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/acp_agents_from_spec_acceptance.rs`**

- [x] `claude_acp_codex_acp_and_opencode_coding_subagents_launch_from_specs_against_tddy_acp_stub_with_todays_command_env_mcp_and_withdrawal`
- [x] `codex_acp_resolves_through_its_codex_dep_and_a_tddyhome_codex_override_moves_both`
- [x] `a_managed_session_still_refuses_codex_acp_because_its_spec_declares_tool_deny_unsupported`

**`tddy-coder`: `packages/tddy-coder/tests/acp_backends_from_spec_acceptance.rs`**

- [x] `tddy_coder_agent_claude_acp_codex_acp_and_opencode_launch_from_specs`

### Unit / integration tests

- [x] `tddy-actions` `resolve`: `sibling_of` resolves next to the dep's resolved binary, and fails naming both when it is absent — `tests/tool_resolve_unit.rs` `a_sibling_binary_resolves_in_the_directory_of_its_deps_resolved_binary`, `a_sibling_binary_absent_beside_its_dep_is_unresolvable_naming_both`.
- [x] `tddy-coding-agent-specs` slots: `AcpSession` produces `mcp_servers`; `AcpMeta` produces the `_meta` object; `ConfigEnv` produces `{"tools":{…:false}}` — `tests/acp_slots_unit.rs` (5 tests).
- [x] `tddy-coding-subagent` confinement from the spec — `tests/spec_confinement_unit.rs` (2 tests).
- [x] `tddy-agent-backend` hooks adapters — `adapters::{acp_hooks, codex_acp_login, opencode_route_file}::tests` (5 tests).

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| ACP launch | Integration against `tddy-acp-stub` | The stub already stands in for ACP agents |
| Resolution, slots | Unit | Pure |

## Decisions & Trade-offs

- **`sibling_of` is a declared tool dep.** It is how "`codex-acp` is beside `codex`" stops being hard-coded, which is the purpose of the user's tool-deps requirement.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-acp-agents-initial-discovery.md`)
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

**Published surface** (bodies `todo!("acp-agents: …")` unless noted):

- `tddy-actions`: `Transport::Acp`; `ToolBinary.sibling_of: Option<String>`; `tool::resolve_sibling`; a `TODO(acp-agents)` in `resolve_closure`.
- `tddy-coding-agent-specs`: `CodingAgentSection.acp: Option<AcpSection>`; `AcpSection { command, env, hooks_adapter, mcp, tool_deny, withdraw_native }` with `run_section()` (real); `CodingAgentSpec::{acp, acp_mcp, acp_tool_deny}` (real accessors); `McpSlot::AcpSession`; `ToolSlot::{AcpMeta { path }, ConfigEnv { env }}` (`ToolSlot::flag` answers `None` for both); `slots::acp::{AcpWithdrawal, acp_mcp_servers, acp_withdrawal}`; `CLAUDE_ACP_SPEC_YAML`, `CODEX_ACP_SPEC_YAML`, both in `builtin_specs()`; `specs/claude-acp.yaml`, `specs/codex-acp.yaml`; `opencode.yaml` gains `coding_agent.acp` only.
- `tddy-agent-backend`: `adapters::acp_hooks::{AcpHooksAdapter, AcpHooksAdapters, AcpHooksLaunch, AcpOpenScope, PreparedAcpHooks}`; `CodexAcpLogin` (`codex-acp-login`), `OpenCodeRouteFile` (`opencode-route-file`).
- `tddy-coding-subagent`: `confinement_of_spec`, `refuse_unless_spec_confinable`; `CodingAgentLauncher::session_meta_for` (a default method returning today's `withdrawal_session_meta`, so every launcher keeps today's answer).
- `tddy-session-lifecycle`: private `acp_plan_from_spec`, `acp_mcp_servers_from_spec`, `acp_session_meta_from_spec`, reached only when `with_coding_agents` is installed (no production path installs it yet) and the harness's transport is ACP; `session_meta_for` overridden.
- `tddy-coder`: `acp_from_spec::{claude_acp_backend_from_spec, codex_acp_backend_from_spec, opencode_acp_backend_from_spec}`; `TODO(acp-agents)` at the three `create_backend` arms.

**Red baseline** (`target/debug/tddy-acp-stub` built with `./dev cargo build -p tddy-acp-stub`):

| Test | Fails on |
|---|---|
| `tddy-actions` sibling tests (2) | `resolve_sibling` — `acp-agents` |
| `tddy-coding-agent-specs` `acp_slots_unit` (5) | `acp_mcp_servers` / `acp_withdrawal` — `acp-agents` (the three YAML files deserialize first) |
| `tddy-agent-backend` adapter tests (5) | `AcpHooksAdapters::builtin`, `CodexAcpLogin::prepare`, `OpenCodeRouteFile::prepare` — `acp-agents` |
| `tddy-coding-subagent` `spec_confinement_unit` (2) | `confinement_of_spec`, `refuse_unless_spec_confinable` — `acp-agents` |
| `tddy-coder` `acp_backends_from_spec_acceptance` (1) | `claude_acp_backend_from_spec` — `acp-agents` |
| `tddy-session-lifecycle` `acp_agents_from_spec_acceptance` (3) | `load_agent_registry` — **`claude-host` (#791)** first; a sequencing fact, as for #795's and #798's lifecycle suites. Past it they reach `acp_plan_from_spec` (`acp-agents`) |

**Pre-existing failures (not this node):** `tddy-build` lib tests do not compile (`builtin.rs` `ToolDef` literal lacks #794's `home`/`files`/`secrets`/`policy`); every other test in `tddy-actions`' resolve/registry suites (`tool-defs`); `tddy-agent-backend` 26 adapter tests and `headless_renderer_port` (n6/n8/n9 todos); `tddy-coding-subagent` `headless_written_files` (`headless-others`); `tddy-session-lifecycle` `headless_*_from_spec_acceptance` (`claude-host`); `tddy-sandbox-app` `agent_from_spec_acceptance` (`jail-agnostic`).

**Deviations from the planned surface:**

- `acp` is a field of `CodingAgentSection` (`extensions.coding_agent.acp`), not of `CodingAgentSpec`; `CodingAgentSpec::acp()` reads it.
- The deny-slot variants are on #791's `ToolSlot` (amendment), not a new `ToolDenySlot`. `ToolSlot` is untagged, so they are written `{ acp_meta: <path> }` and `{ config_env: <VAR> }` — the key is the kind — rather than the PRD's `{ kind: acp_meta, path: … }`, which an untagged struct variant cannot check.
- The MCP kind is `acp-session` (the enum is `kebab-case`, as `claude-mcp-json`), not `acp_session`.
- `AcpSection` carries its own optional `mcp` and `tool_deny` (as #795's `HeadlessSection.mcp` does), so `opencode.yaml`'s ACP variant overrides them without touching its PTY slots, and `withdraw_native` — the tool names `CLAUDE_NATIVE_TOOLS` / `OPENCODE_NATIVE_TOOLS` hold today — so the withdrawn set is spec data.
- `claude-acp.yaml`'s `bunx claude-agent-acp` vs a bare `TDDY_CLAUDE_AGENT_ACP_CLI` binary is a template param, `package_runner`, which the launcher sets when `{binary}` came from `binary.command` rather than `binary.env`. A `ToolBinary` cannot say "fall back to `bunx` with an argument". **Developer decision wanted:** keep the param, or make the default `claude-agent-acp` on `PATH` (dropping `bunx`).
- `codex-acp.yaml`'s route args are `{ splice: route_args }`; the `-m` → `-c model=` rewrite stays `codex_acp_route_args` in Rust until #801 (Boundaries: routing semantics unchanged).
- The hooks adapters have their own registry, `AcpHooksAdapters`, plus an `AcpOpenScope` bracket, because `CodexLoginHooks` needs the request's session directory per open and reports its own failure.
- `session_meta_for` is a new default method on `CodingAgentLauncher`: `tddy-session-agents` builds the `_meta` itself today, and green switches it to ask the launcher (`TODO(acp-agents)`).
- `tddy-sandbox-app`'s `an_unknown_agent_id_is_refused_naming_the_registry_ids` (#799's test) now lists `claude-acp` and `codex-acp`, since this node adds them to the registry. #799's green may instead want `choose_agent` to offer PTY agents only — flagged, not decided here.
- `opencode-route-file` writes the route's config as a file for the daemon launcher too, where today it is inline `OPENCODE_CONFIG_CONTENT`. That is what "the route config as a declared file" means, and green must merge the managed-session withdrawal into `OPENCODE_CONFIG_CONTENT` separately.

**Deletions green makes (items, not files):** `harness_command`, `apply_route`, `opencode_route_config`, `claude_agent_acp_command` and the ACP constants in `coding_agent_launcher.rs`; `ACP_HARNESSES`; `CLAUDE_NATIVE_TOOLS`, `OPENCODE_NATIVE_TOOLS`, `withdrawal_session_meta`, `opencode_withdrawal_config`; `codex_binaries::resolve_codex_acp_binary`.
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
