# Changeset: Headless Cursor, Codex and Oz run from their tool definitions

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — `tddy-agent-backend` (three adapters, two backends, `HeadlessDriver`), `tddy-coding-agent-specs`, `tddy-session-lifecycle` (launcher, conversation close and sweep); Codex OAuth through an adapter
**Stack:** `#agent-tools 9/12` — branch `feature/agent-tools/headless-others`, base `feature/agent-tools/harness-clis`
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

- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `cursor.yaml` and `warp.yaml` gain headless sections; new `codex.yaml`; MCP slot kinds `inline` and `argv`; written-file records
- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): adapters `cursor-stream`, `cursor-models`, `codex-jsonl`, `codex-oauth` and `oz-ndjson`; the `CursorBackend`/`CodexBackend` and `HeadlessDriver` arms render from specs; `HeadlessHarness` removed
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): `coding_agent_launcher.rs` plans from specs; close and sweep remove recorded files
- [ ] [`tddy-coder`](../../../packages/tddy-coder/README.md): `--agent cursor|codex` use specs; `codex_binaries.rs` replaced by the spec's binary resolution

## Responsibility

- `McpSlot::{Inline { flag, key_prefix }, Argv { flag }}` alongside `File`.
- `RenderedSlots.written: Vec<PathBuf>`, recorded at plan time in `headless/<conversation>/written-files`.
- `codex.yaml`:
  - binary `codex`, overridable by `TDDY_CODEX_CLI`;
  - headless `exec -C {worktree} -s {sandbox_mode} --json [resume {thread}] -m {model} {prompt}`;
  - `adapter: codex-jsonl`, `login: { adapter: codex-oauth }`;
  - MCP `inline -c mcp_servers.<n>.*`;
  - `tool_deny: unsupported`.
- `cursor.yaml` headless: `--plan`, `--resume`, `-p`, `--model`, `--output-format stream-json`, `--stream-partial-output`, `--trust` and `--approve-mcps`, with `adapter: cursor-stream` and `models.adapter: cursor-models`.
- `warp.yaml` headless (Oz): `oz agent run -p --output-format ndjson …`, `adapter: oz-ndjson`, MCP `argv --mcp`.
- `HeadlessHarness` and `headless_driver/argv.rs` are deleted.

## Boundaries

- No native-tool withdrawal for these three.
- No ACP (#11).
- The Codex OAuth relay over LiveKit is unchanged. The adapter calls into it.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n6` claude-headless | `HeadlessSection`, `AdapterRegistry`, the headless rendering in both consumers | three more agents through the same path | change the trait or section shape |
| `n7` cursor-cli | `cursor.yaml`, `CursorMcpJson` | adds a headless section to `cursor.yaml` | change its PTY section or hooks |
| `n8` harness-clis | `warp.yaml` | adds Oz's headless section | change the PTY section or placements |
| `n2` claude-host | the slot `render` and `McpSlot::File` | adds `Inline`/`Argv` kinds and the `written` record | change `File`'s behaviour |

## Draft PR contract

The first push publishes the new slot kinds, `written`, `codex.yaml`, the headless sections, the five adapters' registrations and signatures, and the failing tests below. Unimplemented bodies are `// TODO(headless-others): implement`.

## Green wave

**Wave:** 5 of 7
**Greenable independently:** after #6 (headless path), #7 (`cursor.yaml`) and #8 (`warp.yaml`) are green
**Concurrent with:** #10 jail-agnostic
**Blocks:** #11 acp-agents (`codex.yaml` is `codex-acp`'s dep)

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- `backend/cursor.rs` and `backend/codex.rs` build argv. `headless_driver/argv.rs` has `cursor()`, `codex()` and `oz()`, plus `codex_mcp_server_overrides`.
- `coding_agent_launcher.rs:670-733` writes `.cursor/mcp.json` and refuses an existing one.
- `conversation_closed` and `sweep_headless_dirs` do not know that file.
- `HeadlessHarness` enumerates claude, cursor, codex and oz.

## State B

- All four headless agents render from specs and read output through named adapters.
- Files written for a conversation are recorded, and they are removed on close and on sweep.

## Delta

### New
- `packages/tddy-coding-agent-specs/specs/codex.yaml`
- `packages/tddy-agent-backend/src/adapters/{cursor_stream.rs, cursor_models.rs, codex_jsonl.rs, codex_oauth.rs, oz_ndjson.rs}`
### Modified
- `backend/cursor.rs`, `backend/codex.rs`, `headless_driver.rs`.
- `coding_agent_launcher.rs`; `tddy-session-agents` `coding_agent.rs` (close).
- `tddy-coder` `run.rs`, `codex_binaries.rs`.
- Docs at wrap: `docs/ft/coder/managed-codebase-subagents.md`; `packages/tddy-agent-backend/docs/architecture.md`.
### Removed
- `packages/tddy-agent-backend/src/backend/headless_driver/argv.rs`, and the `HeadlessHarness` enum

## Testing Strategy

### Acceptance tests

**`tddy-coder`: `packages/tddy-coder/tests/headless_cursor_codex_from_spec_acceptance.rs`**

- [ ] `tddy_coder_agent_cursor_drives_a_stub_whose_argv_equals_todays_including_resume`
- [ ] `tddy_coder_agent_codex_drives_a_stub_whose_argv_equals_todays_including_the_thread_id`
- [ ] `codexs_login_retry_runs_through_the_codex_oauth_adapter_when_the_stub_reports_an_auth_error`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/headless_subagents_from_spec_acceptance.rs`**

- [ ] `headless_cursor_codex_and_oz_subagents_launch_with_mcp_rendered_as_file_inline_and_argv`
- [ ] `a_closed_cursor_conversation_leaves_no_cursor_mcp_json_and_a_retry_under_the_same_id_is_admitted`
- [ ] `the_startup_sweep_removes_a_recorded_file_of_a_conversation_the_daemon_lost`

### Unit / integration tests

- [ ] `tddy-coding-agent-specs` slots: `Inline` renders `-c mcp_servers.<n>.command=…` with the right escaping; `Argv` renders `--mcp <path>` and writes the file; `written` lists every file.
- [ ] `tddy-agent-backend` adapters: each parses a recorded stream into today's events.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Argv and MCP rendering | Integration with stubs | Behaviour-preserving |
| Adapters | Unit over recordings | Deterministic |
| Close and sweep | Integration on a temp data dir | File lifecycle |

## Decisions & Trade-offs

- **Record what was written rather than infer it at close.** The renderer is the only place that knows, which is the fix the todo proposed.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-headless-others-initial-discovery.md`)
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
