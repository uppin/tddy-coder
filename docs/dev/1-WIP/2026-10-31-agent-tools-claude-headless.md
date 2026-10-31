# Changeset: Headless Claude runs from its tool definition through a named stream adapter

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — two crates (`tddy-agent-backend`, `tddy-coding-agent-specs`) plus the `tddy-session-lifecycle` launcher; two consumers (`CodingBackend`, `HeadlessDriver`); a new adapter-registry contract #9 and #11 build on
**Stack:** `#agent-tools 6/12` — branch `feature/agent-tools/claude-headless`, base `feature/agent-tools/claude-jail`
**PRD:** [PRD-2026-10-31-headless-agents-from-specs.md](../../ft/coder/1-WIP/PRD-2026-10-31-headless-agents-from-specs.md)

## Initial Discovery

[2026-10-31-agent-tools-claude-headless-initial-discovery.md](./2026-10-31-agent-tools-claude-headless-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| `packages/tddy-agent-backend/docs/code-issues/complexity-claude-invoke-sync.md` | ⚠ During. Argv construction leaves `invoke_sync`. Promote to ✅ at wrap if it falls under budget |
| `packages/tddy-agent-backend/docs/code-issues/oversized-file-claude.md` | ⚠ During. `claude.rs` loses the argv and the MCP writer; re-measure at wrap |
| `packages/tddy-agent-backend/docs/code-issues/complexity-claude-process-ndjson-stream.md` | — Unrelated. The parser becomes the adapter unchanged |

## Affected Packages

- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `coding_agent.headless` section; `claude.yaml` gains it
- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): `adapters::{AdapterRegistry, StreamAdapter}` with `claude-stream-json`; `ClaudeBackend` and the `HeadlessDriver` Claude harness render from the spec
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): `DaemonCodingAgentLauncher`'s headless Claude plan reads the spec
- [ ] [`tddy-coder`](../../../packages/tddy-coder/README.md): `create_backend("claude")` passes the registry

## Responsibility

- `tddy_coding_agent_specs::HeadlessSection { argv: Vec<ArgvItem>, env, adapter: String, exit_ok_if_structured: bool }` on `CodingAgentSpec`.
- `tddy_agent_backend::adapters::{AdapterRegistry::builtin(), StreamAdapter (trait: parse turn events from a child's stdout), AdapterRegistry::get(&str)}`, with `claude-stream-json` registered. Spec load refuses an unregistered adapter name.
- `ClaudeBackend::invoke` and `invoke_sync` render argv from `claude.yaml`'s headless section. Goal hints (plan, acceptEdits) map to `permission_mode`.
- `HeadlessDriver`'s Claude arm renders from the spec. `headless_driver/argv.rs::claude()` is deleted.
- `GoalHints.claude_nonzero_exit_ok_if_structured_response` is read from the spec's `exit_ok_if_structured`.

## Boundaries

- Cursor, Codex and Oz headless are unchanged (#9).
- The stream parser's logic is unchanged. It is wrapped, not rewritten.
- `AnyBackend` stays (#12).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` claude-host | `render_argv`, `CodingAgentSpec`, slots `render`, `claude.yaml` | adds a `headless` section and renders it with the same slots | change the template grammar, slot kinds or the PTY section |
| `n1` tool-defs (#790) | registry and resolution | resolves `claude` for headless runs | change resolution |
| `n3`–`n5` | nothing consumed | they sit below because of the line | touch network, jail sections or launchers |

## Draft PR contract

The first push publishes `HeadlessSection`, the `adapters` module (trait, registry, `claude-stream-json` registration), `claude.yaml`'s headless section and the failing tests below. Unimplemented bodies are `// TODO(claude-headless): implement`.

## Green wave

**Wave:** 3 of 7
**Greenable independently:** after #2 is green (rendering and slots). It needs nothing from #3–#5
**Concurrent with:** #5 claude-jail
**Blocks:** #9 headless-others (reuses the headless section and the adapter registry)

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- `tddy-agent-backend/src/backend/claude.rs` builds `-p … --output-format stream-json --verbose`, permission-mode, session-id or resume, append-system-prompt, `--allowedTools`, `--permission-prompt-tool` and `--mcp-config`. It writes the MCP temp file and sets env.
- `headless_driver/argv.rs::claude()` duplicates the argv.
- `stream/claude.rs` and `headless_driver/stream_json.rs` parse.
- `coding_agent_launcher.rs:457` writes the subagent MCP file and approval tool.

## State B

- Both consumers resolve `claude`, render `coding_agent.headless.argv` with goal-derived parameters plus the slots, spawn it, and read output through `AdapterRegistry::get("claude-stream-json")`.
- Argv and parsing are byte-identical, pinned by golden tests.

## Delta

### New
- `packages/tddy-agent-backend/src/adapters/{mod.rs, claude_stream_json.rs}`
### Modified
- `backend/claude.rs`, `headless_driver.rs` (Claude arm), `headless_driver/argv.rs` (`claude()` deleted).
- `tddy-session-lifecycle` `coding_agent_launcher.rs`.
- `tddy-coder` `run.rs`.
- `claude.yaml`, `spec.rs`.
- Docs at wrap: `packages/tddy-agent-backend/docs/architecture.md` § Adapters; `docs/ft/coder/managed-codebase-subagents.md`.

## Testing Strategy

### Acceptance tests

**`tddy-coder`: `packages/tddy-coder/tests/claude_headless_from_spec_acceptance.rs`**

- [ ] `tddy_coder_agent_claude_drives_a_stub_whose_argv_equals_todays_for_the_plan_goal`
- [ ] `the_same_for_accept_edits_and_for_resume`
- [ ] `a_tddyhome_override_of_claude_yaml_changes_the_headless_argv`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/headless_claude_subagent_from_spec_acceptance.rs`**

- [ ] `a_headless_claude_coding_subagent_launches_with_mcp_config_and_approval_tool_rendered_from_the_spec`

**`tddy-coding-agent-specs`: `packages/tddy-coding-agent-specs/tests/spec_load_acceptance.rs` (extended)**

- [ ] `a_spec_naming_an_unregistered_adapter_is_refused_naming_it`

### Unit / integration tests

- [ ] `tddy-agent-backend` `adapters`: the registry lists `claude-stream-json`; the adapter parses a recorded stream into the same events as today's parser; `exit_ok_if_structured` honours a non-zero exit with a structured result.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Argv equality | Integration with a stub binary and golden files | Behaviour-preserving |
| Adapter | Unit over recorded streams | Deterministic |

## Decisions & Trade-offs

- **Adapters are registered by name in `tddy-agent-backend`**, next to the parsers they wrap. Specs name them, and spec load validates the name through a validator the backend registers.
- **`exit_ok_if_structured` moves from a goal hint onto the spec.** It is a property of the agent, not of the goal.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-claude-headless-initial-discovery.md`)
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
