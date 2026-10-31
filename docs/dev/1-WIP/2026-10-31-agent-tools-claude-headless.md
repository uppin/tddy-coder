# Changeset: Headless Claude runs from its tool definition through a named stream adapter

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — two crates (`tddy-agent-backend`, `tddy-coding-agent-specs`) plus the `tddy-session-lifecycle` launcher; two consumers (`CodingBackend`, `HeadlessDriver`); a new adapter-registry contract #798 and #800 build on
**Stack:** `#agent-tools 6/12` — branch `feature/agent-tools/claude-headless`, base `feature/agent-tools/claude-jail`
**PR:** [#795](https://github.com/uppin/tddy-coder/pull/795)
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

- Cursor, Codex and Oz headless are unchanged (#798).
- The stream parser's logic is unchanged. It is wrapped, not rewritten.
- `AnyBackend` stays (#801).

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
**Greenable independently:** after #791 is green (rendering and slots). It needs nothing from #792–#794
**Concurrent with:** #794 claude-jail
**Blocks:** #798 headless-others (reuses the headless section and the adapter registry)

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

- [x] `tddy_coder_agent_claude_drives_a_stub_whose_argv_equals_todays_for_the_plan_goal`
- [x] `the_same_for_accept_edits_and_for_resume`
- [x] `a_tddyhome_override_of_claude_yaml_changes_the_headless_argv`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/headless_claude_subagent_from_spec_acceptance.rs`**

- [x] `a_headless_claude_coding_subagent_launches_with_mcp_config_and_approval_tool_rendered_from_the_spec`

**`tddy-coding-agent-specs`: `packages/tddy-coding-agent-specs/tests/spec_load_acceptance.rs` (extended)**

- [x] `a_spec_naming_an_unregistered_adapter_is_refused_naming_it`

### Unit / integration tests

- [x] `tddy-agent-backend` `adapters`: the registry lists `claude-stream-json`; the adapter parses a recorded stream into the same events as today's parser; `exit_ok_if_structured` honours a non-zero exit with a structured result.

- [x] `tddy-coding-agent-specs` `tests/spec_load_acceptance.rs` (added): `the_same_headless_section_naming_claude_stream_json_loads` (positive control for the refusal), `the_builtin_claude_spec_runs_headless_through_claude_stream_json_and_tolerates_a_structured_exit`.
- [x] `tddy-coding-agent-specs` `tests/headless_render_golden.rs` (added): the built-in headless section, read from the embedded YAML without the registry, renders `build_claude_args`' argv for plan/fresh and acceptEdits/resume and `argv.rs::claude()`'s for a conversation's resumed turn; its one file is the MCP registration; it names `claude-stream-json`; the default tool server is the `tddy-tools` dep; a spec without the section is refused. `src/headless.rs` unit tests: `validate_headless` accepts `claude-stream-json` and refuses an unknown name, naming it and the registered ones.
- [x] `tddy-agent-backend` `tests/headless_renderer_port.rs` (added): a fixture `HeadlessRenderer` drives `ClaudeCodeBackend` and `HeadlessDriver` — the rendered program runs (the backend's own binary does not exist), goal hints map to `permission_mode` (plan / acceptEdits / unset), a resume stays a resume, the rendered files are written before the spawn, the declaration's `exit_ok_if_structured` decides a non-zero exit in both directions, and the driver hands each turn its resume id.

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

- **Contract amendment (commit 3, 2026-10-31), forced by #798 headless-others' red phase.** Every addition is additive.
  - `HeadlessTurn.params: BTreeMap<String, String>` holds agent template parameters, bound as `{name}`: Cursor's `force`, Codex's `sandbox_mode`.
  - `HeadlessTurn.route_args: Vec<String>` holds a provider route's argv, bound as the list `route_args` for `{ splice: route_args }`.
  - `HeadlessSection.mcp: Option<McpSlot>` is a headless MCP slot that overrides `coding_agent.mcp`. Cursor needs it, because headless Cursor registers a project file in the worktree while its PTY session uses `{home}`.
  - `render` is documented to bind `{workdir}` too, since Codex and Oz pass `-C`. Its body is still this node's `todo!`.
  - Test literals in this node's own suites were updated. #798's tests exercise the new bindings; this node's own tests (Claude) leave them empty.

**Published surface (commit 2).**

- `tddy-agent-backend` `src/adapters/mod.rs`: `CLAUDE_STREAM_JSON`, `StreamSinks`, trait `StreamAdapter { name, read_stream, exit_ok, turn_format }`, `AdapterRegistry { builtin (todo), get, contains, names }`, `AdapterError` (re-export of `BackendError` for implementors outside the crate). `src/adapters/claude_stream_json.rs`: `ClaudeStreamJson` (bodies todo; `read_stream` wraps `stream::process_ndjson_stream` unchanged). `src/adapters/headless.rs` (**new, not in the plan**): the port `HeadlessRenderer { agent_id, tool_server, render }`, `HeadlessTurn`, `HeadlessInvocation`, `HeadlessMcpServer`.
- `ClaudeCodeBackend::with_headless_renderer` (builder) and private `invoke_from_spec` (todo); `invoke_sync` takes it only when a renderer is installed, so today's path is untouched. `HeadlessCommand.{renderer, scratch_dir}` and private `HeadlessDriver::run_rendered_turn` (todo), taken only when `renderer` is set. `SessionMode` gains `PartialEq, Eq`.
- `tddy-coding-agent-specs`: `HeadlessSection { argv, env, adapter, exit_ok_if_structured }` + `run_section()`, `CodingAgentSection.headless: Option<_>` (`#[serde(default)]`), `CodingAgentSpec::headless()`; `src/headless.rs`: `validate_headless`, `HeadlessSpecError`, `SpecHeadlessRenderer::new` + its `HeadlessRenderer` impl (todo), `TOOL_SERVER_DEP`. `claude.yaml` gains `coding_agent.headless` (appended after `context_globs`; nothing existing changed). `CodingAgentExtension::validate` gets a `TODO(claude-headless)` comment only.
- `tddy-session-lifecycle`: `DaemonCodingAgentLauncher::with_coding_agents(Arc<HostCodingAgents>)` and private `headless_plan_from_spec` (todo), taken only for `claude` when coding agents are installed.
- `tddy-coder`: `claude_from_spec::claude_backend_from_spec(tools_dir, host)` (todo; new module rather than `run.rs`); `create_backend("claude")` carries a `TODO(claude-headless)`; new path deps `tddy-actions`, `tddy-coding-agent-specs`.

**Red baseline.**

- Failing on this node's `todo!`: `adapters` unit 3/3 (`AdapterRegistry::builtin`), `claude_stream_json` unit 6/7 (`read_stream`, `exit_ok`, `turn_format`); `headless_renderer_port` 8/8 (`invoke_from_spec`, `run_rendered_turn`); `headless_render_golden` 6/6 and `headless::tests` 2/2 (`AdapterRegistry::builtin` is the first todo they reach — the YAML parse before it succeeds, so the built-in headless section deserializes); `claude_headless_from_spec_acceptance` 3/3 (`claude_backend_from_spec`).
- `todays_parser_reads_the_recorded_stream_into_a_result_a_session_a_question_and_progress` **passes**: it is the positive control that the recorded stream is not empty of events. It pins that today's parser keeps assistant text ahead of the result text (`"Reading done.The plan is ready."`).
- **Fail in their Given on a parent's todo** (sequencing, not this node's bug): `headless_claude_subagent_from_spec_acceptance` 1/1 and the 3 new `spec_load_acceptance` tests stop at `todo!("claude-host: load_agent_registry")` (#791). Green of these needs #791 and #790 (`resolve_closure`, `render_argv`, `slots::render`) green.

**Pre-existing failures seen (not this node's).** `tddy-agent-backend` `acp_driver_acceptance` 10, `acp_driver_stream_acceptance` 2, `claude_acp_on_driver_acceptance` 2: `target/debug/tddy-acp-stub` is not built by a scoped `cargo test` (harness; files unchanged from master). `tddy-coding-agent-specs` lib 8 and `spec_load_acceptance` 4 (`claude-host` todos). `tddy-session-lifecycle`: see the scoped run recorded in the PR report (`sandboxed_*` suites fail on this Mac — no sandbox RPC bridge / unbuilt binaries).

**Deviations from the plan.**

- **No dependency cycle, no validator hook.** `tddy-coding-agent-specs` already depends on `tddy-agent-backend`, so adapter names are checked by calling `AdapterRegistry::builtin()` directly from `validate_headless` (and, at green, `CodingAgentExtension::validate`). The cycle is the *other* direction: `ClaudeCodeBackend` and `HeadlessDriver` cannot name `CodingAgentSpec`. So the backend owns a port, `adapters::HeadlessRenderer`, and the specs crate implements it (`SpecHeadlessRenderer`); `tddy-coder` and the launcher wire the two together. The Decision "a validator the backend registers" is replaced by this.
- `StreamAdapter::turn_format() -> Option<HeadlessHarness>` names which of the driver's existing transcripts (`stream_json.rs`) decodes a conversation turn: the driver's parser is wrapped by selection, not moved behind the trait. #798 may fold `Transcript` behind the trait instead.
- The MCP config moves: the backend's goes to `<session_dir>/claude-mcp-config.json` (the MCP slot's file name) instead of a temp file; the driver's to `<data>/headless/<conv>/claude-mcp-config.json` instead of `MCP_CONFIG_FILE`. Flag and position are unchanged; the goldens pin the new paths. The tool server for the backend is the spec's resolved `tddy-tools` dep (`HeadlessRenderer::tool_server`), not `tddy_tools_path()` beside the executable.
- Acceptance test paths and names are as planned; `tddy-agent-backend`'s unit tests are in `src/adapters/*` plus a new integration file `tests/headless_renderer_port.rs`.

**For green (developer decisions flagged).**

- `claude.yaml` sets `exit_ok_if_structured: true` for the agent, per the Decision. Today only goals whose hints set `claude_nonzero_exit_ok_if_structured_response` (plan) tolerate a non-zero exit; with the spec deciding, **every** Claude goal will. Confirm, or make the template's flag combine with the goal hint.
- The slot renderer orders deny flags before `--permission-prompt-tool`/`--mcp-config`; `argv.rs::claude()` puts `--disallowedTools` last. Byte-identity holds only because a headless Claude conversation is never managed (no deny flags today). If that changes, the driver's order changes with it.
- Owner-only (0600) mode for the driver's MCP file is now the consumer's job when it writes `HeadlessInvocation::files`; the acceptance test pins it.
- Deletions green makes (TODO-marked, need approval as deletions of code, not files): `build_claude_args`, `create_mcp_config_temp_file`, `headless_driver/argv.rs::claude()`, the launcher's hand-built Claude `mcp_servers_wrapped` registration.
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
