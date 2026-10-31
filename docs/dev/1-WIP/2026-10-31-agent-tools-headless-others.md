# Changeset: Headless Cursor, Codex and Oz run from their tool definitions

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — `tddy-agent-backend` (three stream adapters and a models adapter, two backends, `HeadlessDriver`), `tddy-coding-agent-specs` (two new specs, three MCP slot kinds), `tddy-coding-subagent` + `tddy-session-lifecycle` (launcher, written-file record, conversation close and sweep)
**Stack:** `#agent-tools 9/12` — branch `feature/agent-tools/headless-others`, base `feature/agent-tools/harness-clis`
**PR:** [#798](https://github.com/uppin/tddy-coder/pull/798)
**PRD:** [PRD-2026-10-31-headless-cursor-codex-oz-from-specs.md](../../ft/coder/1-WIP/PRD-2026-10-31-headless-cursor-codex-oz-from-specs.md)

## Initial Discovery

[2026-10-31-agent-tools-headless-others-initial-discovery.md](./2026-10-31-agent-tools-headless-others-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-10-25-cursor-project-mcp-config-outlives-its-conversation.md](../todo/2026-10-25-cursor-project-mcp-config-outlives-its-conversation.md) | ✅ **RESOLVED HERE.** The MCP slot renderer records every file it writes under `headless/<conversation>/`, and `conversation_closed` and `sweep_headless_dirs` remove what was recorded |
| [2026-10-25-headless-cursor-codex-oz-cannot-withdraw-native-tools.md](../todo/2026-10-25-headless-cursor-codex-oz-cannot-withdraw-native-tools.md) | ⚠ During. Unchanged: each spec declares `tool_deny: unsupported`, and `confinement_for` reads that |
| `packages/tddy-agent-backend/docs/code-issues/oversized-file-cursor.md`, `complexity-cursor-process-cursor-stream.md` | ⚠ During. `cursor.rs` loses its argv and MCP writer, and the parser is wrapped unchanged |
| `packages/tddy-agent-backend/docs/code-issues/complexity-headless-driver-run-turn.md` | ⚠ During. `run_turn` gains no branches; the harness arms disappear |

## Affected Packages

- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `cursor.yaml` gains a headless section; new `codex.yaml` and `oz.yaml`; MCP slot kinds `inline`, `argv` and `workdir`; `RenderedSlots.written`
- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): adapters `cursor-stream`, `cursor-models`, `codex-jsonl` and `oz-ndjson`; the `CursorBackend`/`CodexBackend` and `HeadlessDriver` arms render from specs (`HeadlessHarness` stays — see Decisions)
- [ ] [`tddy-coding-subagent`](../../../packages/tddy-coding-subagent/README.md): the `written-files` record, `remove_recorded_files`, `close_headless_conversation`; the sweep removes recorded files
- [ ] [`tddy-session-agents`](../../../packages/tddy-session-agents/README.md): `AgentRoster::conversation_closed` closes through `close_headless_conversation`
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): `coding_agent_launcher.rs` plans every headless harness from specs and records what it will write
- [ ] [`tddy-coder`](../../../packages/tddy-coder/README.md): `--agent cursor|codex` use specs; `codex_binaries.rs` replaced by the spec's binary resolution

## Responsibility

- `McpSlot::{Inline { flag, key_prefix }, Argv { flag }, Workdir { path, format }}` alongside `File`/`Placed`/`Unsupported`; renderers `slots::headless_mcp::{render_inline_mcp, render_argv_mcp, render_workdir_mcp, mcp_written_paths, ARGV_MCP_FILE}`.
- `RenderedSlots.written: Vec<PathBuf>`, recorded at plan time in `headless/<conversation>/written-files` (`tddy_coding_subagent::launcher::{HEADLESS_WRITTEN_FILES, record_written_files, remove_recorded_files, close_headless_conversation}`).
- `codex.yaml` (`CODEX_SPEC_YAML`, in `builtin_specs()`):
  - binary `codex`, overridable by `TDDY_CODEX_CLI`;
  - headless `exec {slots} -C {workdir} -s {sandbox_mode|workspace-write} --json [resume] {route_args} [-m {model}] [{thread}] {prompt}` — the backend's argv exactly;
  - `adapter: codex-jsonl`, `exit_ok_if_structured: true`;
  - MCP `{ kind: inline, flag: -c, key_prefix: mcp_servers }`;
  - `tool_deny: unsupported`; no `network:` key; `placements: [host]`.
- `cursor.yaml` headless (a new `coding_agent.headless` key; no existing section changed): `{plan}→--plan`, `--resume`, `-p`, `--model`, `--output-format stream-json`, `--stream-partial-output`, `{force}→--force`, `--trust`, `--approve-mcps`; `adapter: cursor-stream`; `headless.mcp: { kind: workdir, path: .cursor/mcp.json, format: cursor-mcp-json }`.
- `oz.yaml` (`OZ_SPEC_YAML`, in `builtin_specs()`; `warp.yaml` untouched): binary `oz` (`TDDY_OZ_CLI`), headless `oz agent run -p … --output-format ndjson -C {workdir} [--conversation] [--model] {slots} [--profile]`, `adapter: oz-ndjson`, MCP `{ kind: argv, flag: --mcp }`, `placements: [host]`.
- Adapters `CursorStream`/`CURSOR_STREAM`, `CodexJsonl`/`CODEX_JSONL`, `OzNdjson`/`OZ_NDJSON` (stream) and `CursorModels`/`CURSOR_MODELS` (models), each wrapping today's parser.
- `CursorBackend::with_headless_renderer`, `CodexBackend::with_headless_renderer` (and their `invoke_from_spec`); `tddy_coder::headless_from_spec::{cursor_backend_from_spec, codex_backend_from_spec}`.
- `headless_driver/argv.rs` is deleted at green. `HeadlessHarness` is **not** (see Decisions).

## Boundaries

- No native-tool withdrawal for these three.
- No ACP (#800).
- The Codex OAuth relay over LiveKit is unchanged. The adapter calls into it.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n6` claude-headless | `HeadlessSection` (+ `mcp`, amendment `c900cf5cf`), `HeadlessTurn` (+ `params`, `route_args`), `{workdir}` bound by `render`, `AdapterRegistry`, the headless rendering in both consumers | three more agents through the same path; `params` carry Cursor's `plan`/`force`, Codex's `sandbox_mode`, Oz's `profile`; `route_args` Codex's route | change the trait or section shape |
| `n7` cursor-cli | `cursor.yaml`, `CursorMcpJson` | adds a headless section to `cursor.yaml` | change its PTY section or hooks |
| `n8` harness-clis | `LaunchAdapters`, `ModelsAdapter` | registers `cursor-models` beside `opencode-models` | touch `warp.yaml` (Oz is its own `oz.yaml`) |
| `n2` claude-host | the slot `render`, `McpSlot::File`, `load_agent_registry` | adds `Inline`/`Argv`/`Workdir` kinds and the `written` record | change `File`'s behaviour |

## Draft PR contract

The first push publishes the new slot kinds, `written`, `codex.yaml`, the headless sections, the five adapters' registrations and signatures, and the failing tests below. Unimplemented bodies are `// TODO(headless-others): implement`.

## Green wave

**Wave:** 5 of 7
**Greenable independently:** after #791 (`load_agent_registry`, `slots::render`), #795 (headless path, `SpecHeadlessRenderer::render`, `AdapterRegistry::builtin`), #796 (`cursor.yaml`) and #797 (`LaunchAdapters::builtin`) are green
**Concurrent with:** #799 jail-agnostic
**Blocks:** #800 acp-agents (`codex.yaml` is `codex-acp`'s dep)

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n2, n6 → n9      n7, n8 → n9      n7 → n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- `backend/cursor.rs` and `backend/codex.rs` build argv. `headless_driver/argv.rs` has `cursor()`, `codex()` and `oz()`, plus `codex_mcp_server_overrides`.
- `coding_agent_launcher.rs:670-733` writes `.cursor/mcp.json` and refuses an existing one.
- `conversation_closed` and `sweep_headless_dirs` do not know that file.
- `HeadlessHarness` enumerates claude, cursor, codex and oz.

## State B

- All four headless agents render from specs and read output through named adapters; Oz from its own `oz.yaml`.
- Files written for a conversation are recorded, and they are removed on close and on sweep.

## Delta

### New
- `packages/tddy-coding-agent-specs/specs/{codex.yaml, oz.yaml}`; `src/slots/headless_mcp.rs`
- `packages/tddy-agent-backend/src/adapters/{cursor_stream.rs, cursor_models.rs, codex_jsonl.rs, oz_ndjson.rs}`
- `packages/tddy-coder/src/headless_from_spec.rs`
### Modified
- `cursor.yaml` (adds `coding_agent.headless`); `spec.rs` (`McpSlot` variants); `slots/mod.rs` (`RenderedSlots.written`); `lib.rs` (`builtin_specs`).
- `backend/cursor.rs`, `backend/codex.rs`, `headless_driver.rs`, `adapters/{mod,launch}.rs` (registration TODOs).
- `tddy-coding-subagent` `launcher.rs` (record, close, sweep); `tddy-session-agents` `coding_agent.rs` (close).
- `coding_agent_launcher.rs` (every harness through `headless_plan_from_spec` once coding agents are installed).
- `tddy-coder` `run.rs`, `codex_binaries.rs` (green).
- Docs at wrap: `docs/ft/coder/managed-codebase-subagents.md`; `packages/tddy-agent-backend/docs/architecture.md`.
### Removed (at green)
- `packages/tddy-agent-backend/src/backend/headless_driver/argv.rs`; `build_cursor_cli_args`, `register_cursor_mcp_config`, `build_codex_exec_argv`; the launcher's `headless_program`, `mcp_servers_by_name`, `write_cursor_project_mcp_config`, `OZ_BINARY`. **Not** `HeadlessHarness`.

## Testing Strategy

### Acceptance tests

**`tddy-coder`: `packages/tddy-coder/tests/headless_cursor_codex_from_spec_acceptance.rs`**

- [x] `tddy_coder_agent_cursor_drives_a_stub_whose_argv_equals_todays_including_resume`
- [x] `tddy_coder_agent_codex_drives_a_stub_whose_argv_equals_todays_including_the_thread_id`
- [x] `a_codex_auth_error_still_carries_todays_sign_in_remediation_through_the_spec` — a **preserved-behaviour pin**: today's `codex_openai_auth_remediation` hint is still appended. It replaces the planned login-retry test, which would have pinned behaviour that does not exist (no retry on this path; the only one is `codex-acp`'s, #800).

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/headless_subagents_from_spec_acceptance.rs`**

- [x] `headless_cursor_codex_and_oz_subagents_launch_with_mcp_rendered_as_file_inline_and_argv`
- [x] `a_closed_cursor_conversation_leaves_no_cursor_mcp_json_and_a_retry_under_the_same_id_is_admitted`
- [x] `the_startup_sweep_removes_a_recorded_file_of_a_conversation_the_daemon_lost`

### Unit / integration tests

- [x] `tddy-coding-agent-specs` `src/slots/headless_mcp.rs` (5): `Inline` renders `-c mcp_servers.<n>.command=…` with TOML escaping and names env vars only; `Argv` renders `--mcp <path>` and the servers-by-name file; `Workdir` writes `<workdir>/.cursor/mcp.json`; `mcp_written_paths` lists every kind's files.
- [x] `tddy-coding-agent-specs` `tests/headless_others_render_golden.rs` (13): Cursor (backend plan+resume, conversation, worktree registration), Codex (read-only plan, resume, provider route, conversation with inline server), Oz (resumed turn, `--mcp` file), and each spec's MCP slot.
- [x] `tddy-agent-backend` adapters (`cursor_stream.rs` 4, `codex_jsonl.rs` 5, `oz_ndjson.rs` 3, `cursor_models.rs` 2): each reads a recorded stream into today's result; three are positive controls on today's parser and pass.
- [x] `tddy-coding-subagent` `tests/headless_written_files.rs` (7): record, remove, close, sweep, another daemon's file left alone.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Argv and MCP rendering | Integration with stubs | Behaviour-preserving |
| Adapters | Unit over recordings | Deterministic |
| Close and sweep | Integration on a temp data dir | File lifecycle |

## Decisions & Trade-offs

- **Record what was written rather than infer it at close.** The renderer is the only place that knows, which is the fix the todo proposed.
- **Oz is its own `oz.yaml`, not a variant in `warp.yaml`** (decided 2026-10-31). `warp.yaml`'s binary is `warp` and its MCP slot `unsupported`; Oz runs another binary with an `--mcp` file. A tool definition is one binary, and `warp.yaml` belongs to #797.
- **Cursor's headless MCP is `headless.mcp: { kind: workdir }`**, through the n6 amendment's `HeadlessSection.mcp`: both of today's headless writers put it in the worktree, while `cursor.yaml`'s PTY slot (#796) places it in `{home}`.
- **Agent-specific argv bits are turn `params`** (n6 amendment): Cursor's `plan`/`force` (the backend binds `force`, a conversation does not — today's split), Codex's `sandbox_mode` (unbound renders `workspace-write`), Oz's `profile`. Codex's provider route is `route_args`.
- **Codex's `-m` follows `--json`** for both consumers. The backend's argv is matched exactly; a headless conversation's `-m` moves from before `--json` to after it — the same meaning, one template.
- **`HeadlessHarness` stays.** `StreamAdapter::turn_format` (#795) returns it; #801 agent-registry collapses the enums.
- **No `codex-oauth` adapter.** `CodexBackend` has no login retry today, only the remediation hint; the retry lives in `codex-acp` (#800).

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-headless-others-initial-discovery.md`)
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

**Red baseline (2026-10-31, scoped: `tddy-coding-agent-specs`, `tddy-agent-backend`, `tddy-coding-subagent`, `tddy-session-agents`, `tddy-session-lifecycle`, `tddy-coder`).**

- Fail on this node's `todo!("headless-others: …")`:
  - `headless_mcp` unit tests (5);
  - the four adapters' tests (11, with 3 positive controls passing);
  - `headless_written_files.rs` (7);
  - the spec-slot tests in `headless_others_render_golden.rs` (4);
  - `headless_cursor_codex_from_spec_acceptance.rs` (3, on `cursor_backend_from_spec`/`codex_backend_from_spec`). The remediation pin runs today's `CodexBackend` first as its positive control.
- Fail first on a parent's TODO (sequencing, not this node's bug):
  - the 9 argv/registration goldens in `headless_others_render_golden.rs` (`AdapterRegistry::builtin`, #795; then `SpecHeadlessRenderer::render`, #795);
  - all 3 of `headless_subagents_from_spec_acceptance.rs` (`load_agent_registry`, #791; then `headless_plan_from_spec`, #795; then this node's record/close).

**Pre-existing failures (identical test files fail before this change):**
- `tddy-coding-subagent` ACP suites (`coding_agent_session_acceptance`, `conversation_updates_acceptance`, `main_agent_answer_acceptance`, `permission_defer/frames/yield_acceptance`) need `target/debug/tddy-acp-stub`.
- The parents' red tests stay red as they were (`headless_renderer_port.rs`, `headless_render_golden.rs`, `claude_headless_from_spec_acceptance.rs`, the specs crate's n2/n7/n8 unit and acceptance tests).

Existing green tests in the touched packages still pass:
- `tddy-agent-backend` `headless_driver_acceptance` (11), `codex_route_acceptance` (2) and lib (119);
- `tddy-session-lifecycle` `coding_agent_launcher` lib tests (42);
- `tddy-coding-subagent` `sweep_headless_dirs` (5) and the other green suites.

**Deviations from the planned surface, and why:**
- **`oz.yaml` instead of a `warp.yaml` variant.** Developer decision 2026-10-31.
- **A third MCP kind, `McpSlot::Workdir`,** for Cursor's headless worktree file. `Placed` binds only `{home}`.
- **`codex-oauth` is dropped, and its acceptance test is restated as the remediation pin** (decision 4).
- **`HeadlessHarness` deletion is dropped** (decision 3).
- **The launcher's spec guard is widened** from Claude to every harness when coding agents are installed. That path is taken only with `with_coding_agents`, which no green test sets.
- **`backend::cursor` became `pub(crate)`,** so `cursor-models` can wrap `parse_cursor_model_list` unchanged.
- **`RenderedSlots.written` forced edits to three n2 test literals** in `slots/mod.rs` (decision 7): `written: vec![config_path]` for the MCP-file test, empty for the other two.
- **No registry-membership tests for the new adapters.** `AdapterRegistry::builtin`/`LaunchAdapters::builtin` are parent TODOs, so the registration is a TODO in their bodies.

**For green (needs the developer):**
- n6's `the_builtin_registry_lists_claude_stream_json` and n8's `the_builtin_launch_adapters_are_opencode_models_and_warp_resume_token` assert exact lists. They must widen at this node's green, when `cursor-stream`, `codex-jsonl`, `oz-ndjson` and `cursor-models` register (decision 6). Not edited now.
- `cursor.yaml`'s `models` section needs `list_command: ["{binary}", --list-models]` and `adapter: cursor-models` for `cursor-models` to be named by a spec. That edits a section #796 owns, so it needs consent. It is a TODO comment in `cursor.yaml` until then.
- `AgentRoster::conversation_closed` must delegate to `close_headless_conversation`. No test drives the roster itself; the acceptance test calls `close_headless_conversation`.
- Deletions at green, which per CLAUDE.md need a request first: `headless_driver/argv.rs` and the hand-built builders listed under Delta → Removed.
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
