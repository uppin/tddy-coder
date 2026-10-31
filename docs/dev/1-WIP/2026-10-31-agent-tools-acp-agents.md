# Changeset: ACP coding agents launch from their tool definitions

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — `tddy-coding-agent-specs`, `tddy-agent-backend` (ACP backends, hooks adapters), `tddy-session-lifecycle` (`DaemonCodingAgentLauncher`), `tddy-coding-subagent` (confinement); tool deps drive binary resolution
**Stack:** `#agent-tools 11/12` — branch `feature/agent-tools/acp-agents`, base `feature/agent-tools/jail-agnostic`
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

- `Transport::Acp` and `CodingAgentSpec.acp: AcpSection { command: Vec<ArgvItem>, hooks_adapter: Option<String> }`.
- `McpSlot::AcpSession` and `ToolDenySlot::{Flag, AcpMeta { path }, ConfigEnv { env }, Unsupported}`.
- `claude-acp.yaml`: `bunx claude-agent-acp`, overridable by `TDDY_CLAUDE_AGENT_ACP_CLI`; `tool_deny: acp_meta claudeCode.options.disallowedTools`.
- `codex-acp.yaml`: `deps: [codex]`. The binary resolves from the dep's install. Route args are a template, `hooks_adapter: codex-acp-login`, `tool_deny: unsupported`.
- `opencode.yaml` gains an `acp` variant: `opencode acp`, `tool_deny: config_env OPENCODE_CONFIG_CONTENT`, the route config as a declared file.
- `confinement_for(spec)` derives `CannotWithdraw` from `tool_deny: Unsupported`.

## Boundaries

- The ACP driver protocol is unchanged.
- No new ACP agents.
- Routing semantics are unchanged (#12 moves route compatibility into spec data).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n9` headless-others | `codex.yaml`, `McpSlot::{Inline, Argv}` | `codex-acp` depends on `codex` | change `codex.yaml`'s headless section |
| `n8` harness-clis | `opencode.yaml`, `route` slot | adds the ACP variant | change the PTY section |
| `n6` claude-headless | `AdapterRegistry` | registers the hooks adapters | change the trait |
| `n1` tool-defs (#790) | `deps` and `resolve_closure` | `codex-acp` resolves through its dep | change resolution |

Resolving a binary **from a dep's install** needs a resolution source on #1's `ToolBinary`: `sibling_of: <dep id>`. It is additive, and it is the only `tddy-actions` resolution change in this node.

## Draft PR contract

The first push publishes `Transport::Acp`, `AcpSection`, the new slot variants, `ToolBinary.sibling_of`, the two specs and the variant, the hooks-adapter registrations, and the failing tests below. Unimplemented bodies are `// TODO(acp-agents): implement`.

## Green wave

**Wave:** 6 of 7
**Greenable independently:** after #9 (`codex.yaml`) and #8 (`opencode.yaml`) are green
**Concurrent with:** none
**Blocks:** #12 agent-registry

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

- [ ] `claude_acp_codex_acp_and_opencode_coding_subagents_launch_from_specs_against_tddy_acp_stub_with_todays_command_env_mcp_and_withdrawal`
- [ ] `codex_acp_resolves_through_its_codex_dep_and_a_tddyhome_codex_override_moves_both`
- [ ] `a_managed_session_still_refuses_codex_acp_because_its_spec_declares_tool_deny_unsupported`

**`tddy-coder`: `packages/tddy-coder/tests/acp_backends_from_spec_acceptance.rs`**

- [ ] `tddy_coder_agent_claude_acp_codex_acp_and_opencode_launch_from_specs`

### Unit / integration tests

- [ ] `tddy-actions` `resolve`: `sibling_of` resolves next to the dep's resolved binary, and fails naming both when it is absent.
- [ ] `tddy-coding-agent-specs` slots: `AcpSession` produces `mcp_servers`; `AcpMeta` produces the `_meta` object; `ConfigEnv` produces `{"tools":{…:false}}`.

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
