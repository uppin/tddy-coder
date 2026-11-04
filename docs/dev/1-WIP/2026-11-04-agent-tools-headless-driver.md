# Changeset: One headless driver, configured by the spec, replaces the Claude, Codex and Cursor workflow backends

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Refactor (behaviour-preserving, with named, decided behaviour changes)
**Complexity**: 5 — several row-4 signals: crates `tddy-agent-backend`, `tddy-coding-agent-specs` (new `headless` fields in 4 YAMLs), `tddy-coder`, `tddy-session-lifecycle`, `tddy-integration-tests`; a spawned-process seam whose cancellation moves from a global pid slot to process groups (concurrency); ~2,500 lines of three backends collapsed; byte-for-byte argv/MCP/error-text pins across ~15 suites; the spec-field set is not settled
**Stack:** `#agent-tools 15/19` — branch `feature/agent-tools/headless-driver`, base `feature/agent-tools/cli-spec-features` (#820)
**PRD:** none of its own. Serves the registry PRD on `feature/agent-tools/agent-registry` (`docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`): an agent is a spec plus adapters, not a Rust backend

## Initial Discovery

[2026-11-04-agent-tools-headless-driver-initial-discovery.md](./2026-11-04-agent-tools-headless-driver-initial-discovery.md) — Exploration 2 is this node's own discovery.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-02-claude-hand-built-headless-paths.md](../todo/2026-11-02-claude-hand-built-headless-paths.md) | ✅ **RESOLVED HERE.** `build_claude_args`, `create_mcp_config_temp_file`, `argv.rs::claude()` go with the bespoke backend; their tests move onto the spec renderer. The launcher's hand-built `mcp_servers_wrapped` goes only if no agentless-launcher test still needs it — otherwise it stays with [2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md](../todo/2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md) and this record is narrowed, not deleted |
| [2026-11-03-headless-others-legacy-argv-deletion.md](../todo/2026-11-03-headless-others-legacy-argv-deletion.md) | ✅ **RESOLVED HERE.** `headless_driver/argv.rs`, the `renderer: None` branch, `build_cursor_cli_args`, `register_cursor_mcp_config`, `build_codex_exec_argv`, and the launcher's `write_cursor_project_mcp_config`, `mcp_servers_by_name`, `headless_program`, `OZ_BINARY` |
| [2026-10-25-headless-cursor-codex-oz-cannot-withdraw-native-tools.md](../todo/2026-10-25-headless-cursor-codex-oz-cannot-withdraw-native-tools.md) | — Reference. Unchanged: needs per-CLI proof against a real binary |
| `packages/tddy-agent-backend/docs/code-issues/complexity-claude-run-child.md`, `oversized-file-claude.md`, `oversized-file-cursor.md`, `oversized-file-codex.md`, `oversized-file-headless-driver.md` | ⚠ During. Deleting the three backends and `argv.rs` should close them; re-measure at wrap and delete the records the measurement closes. The generic driver's own size is measured fresh |
| `complexity-claude-process-ndjson-stream.md`, `complexity-cursor-process-cursor-stream.md`, `complexity-headless-driver-run-turn.md` | — Reference. The stream parsers stay as adapters; restructuring is the sweep node's |

## Affected Packages

- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): a generic headless `CodingBackend` over the spec renderer and the stream adapter; `StreamAdapter` hooks; `HeadlessHarness` collapses; the three backends' turn code deleted
- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): new `coding_agent.headless` fields (below), declared in `claude.yaml`, `cursor.yaml`, `codex.yaml`, `oz.yaml`
- [ ] [`tddy-coder`](../../../packages/tddy-coder/README.md): `create_backend`'s claude/cursor/codex arms build the generic driver; `claude_from_spec.rs` folds into `headless_from_spec.rs`
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): the launcher's headless subagent plan stops matching `HeadlessHarness`
- [ ] [`tddy-coding-subagent`](../../../packages/tddy-coding-subagent/README.md), [`tddy-session-agents`](../../../packages/tddy-session-agents/README.md): `HeadlessHarness::from_id` callers
- [ ] `tddy-integration-tests`: backend suites move onto the spec renderer

## Responsibility

- **One generic headless workflow backend** (`impl CodingBackend`), configured by the agent's spec: argv from `SpecHeadlessRenderer`, stdout decoded by the spec's stream adapter, async spawn in its own process group with `killpg` on cancel, a stderr grace and a **turn deadline**. It shares the spawn/cancel/stderr core with `HeadlessDriver` (the conversation driver) instead of a fourth copy.
- **Protocol differences become spec fields**, each named after what it does, never after an agent (exact names settled at red): system-prompt delivery (`flag` | `prepend`); goal-hint parameters (plan mode, read-only, accept-edits, always-on); whether the approval tool is bound; whether a route replaces the model; whether the workflow registers the tool server; who mints a fresh session id (`caller` | `agent`); templated env values (`{session_dir}`, `{self_exe}`); scratch directory (`session_dir` | `workdir`). `claude.yaml`, `cursor.yaml`, `codex.yaml`, `oz.yaml` declare them.
- **Small named stream adapters where unavoidable**: `StreamAdapter::failure_detail(stream, stderr)` and `keeps_stderr`, implemented by the existing `claude-stream-json`, `cursor-stream`, `codex-jsonl`, `oz-ndjson` adapters; `turn_format() -> Option<HeadlessHarness>` becomes a transcript factory on the adapter, so the closed `HeadlessHarness` enum is deleted.
- `list_models` reads the spec's `models` section (`list_command` + its adapter) with the **spec-resolved** binary and a deadline.
- `name()` returns the spec id, so the workflow engine's codex thread-id swap and the context-glob/model tables keep working unchanged.
- `create_backend("claude" | "cursor" | "codex")` builds the generic backend. `oz` becomes buildable by the same code (it is listed by `backend_agent_ids` but has no arm); wiring `--agent oz` and every other registry id end-to-end is n16's.
- Delete the three bespoke turn paths and the hand-built argv builders (see **Removed**).

## Boundaries

- Does **not** replace `AnyBackend`, `create_backend` or `BUILTIN_BACKEND_AGENT_IDS`: `feature/agent-tools/spec-backend` (n16). Here `AnyBackend::{Claude, Cursor, Codex}` arms hold the generic backend.
- Does not touch ACP backends or `AcpDriver`.
- Keeps `spawn_oauth_login`, `CODEX_*_FILENAME`, `write_codex_thread_id_file`, `transcript_usage`, `which_binary`/`tddy_tools_path`, `resolve_codex_binary` (codex-acp, OAuth).
- No CLI-session (PTY) change; no daemon.yaml change.
- No file split beyond deleting the bespoke code; the stream parsers' complexity stays recorded for the sweep node.

**Stack-wide rules this node keeps:**
- The agent process sees no `TDDY_*` variable. Today all three backends set `TDDY_SOCKET`, `TDDY_REPO_DIR`, `TDDY_SESSION_DIR`, and codex sets `TDDY_CODEX_OAUTH_OUT`. **Open — confirm at red:** the generic driver passes these to `tddy-tools` through the tool-server slot's own env rather than the agent's, or the rule is relaxed for the workflow backend; the decision is recorded, not defaulted.
- No hangs: turn deadline, stderr grace, `list_models` deadline; the global `CHILD_PID` slot goes.
- No fallback: a missing tool server is an error, never a silently dropped MCP config (claude.rs:465 today); no process-cwd fallback for the workdir.
- Deletions not listed in **Removed** need the developer's approval.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n13` route-support (#819) | `RoutedAgent`, agent-keyed `RouteRefusal`, route bodies by (shape, provider) | the generic backend's route is resolved from the spec's `RoutedAgent`; a spec without `route` refuses a route | add route bodies or `RouteRefusal` variants |
| `n14` cli-spec-features (#820) | (line predecessor only) | nothing — no real edge | — |
| `n6` claude-headless (#795), `n9` headless-others (#798) | `SpecHeadlessRenderer`, `HeadlessSection`, the four stream adapters, the goldens | renders every turn through them | change the renderer's argv output for existing params |

## Draft PR contract

The first push of this PR publishes: the generic backend type and constructor (`from_spec(spec, host, progress)`), the new `HeadlessSection` fields and their YAML lines, the `StreamAdapter` hooks and transcript factory, and the failing tests below. Unimplemented bodies are `// TODO(headless-driver): implement`.

## Green wave

**Wave:** 2 of 5
**Greenable independently:** yes, once #819 is green — tests run stub binaries through the spec renderer; nothing needs n14's launch behaviour
**Concurrent with:** `feature/agent-tools/cli-spec-features` (#820)
**Blocks:** `feature/agent-tools/spec-backend` (n16: `SpecBackend::Headless` wraps this driver)

Real dependency edges, as opposed to the branch line:

    n12 → n13      n13 → n14, n15, n16      n15 → n16      n14 → n17      n17 → n18      n16, n18 → n19

## State A

- `ClaudeCodeBackend` (`backend/claude.rs`, 859), `CursorBackend` (`cursor.rs`, 822), `CodexBackend` (`codex.rs`, 843) each run a blocking `run_child` (~90% identical: claude :495-782, cursor :368-632, codex :268-505) and map goal hints to params in Rust.
- Cancellation is one global `CHILD_PID` (`mod.rs:172-198`); stderr `join()` and `wait()` have no deadline.
- `HeadlessDriver` (`headless_driver.rs`) is the conversation driver, not a `CodingBackend`; `HeadlessHarness {Claude, Cursor, Codex, Oz}` (:46-77) is a closed enum chosen by `turn_format()`.
- Hand-built argv still compiles for tests: `build_claude_args` :77, `build_cursor_cli_args` :285, `build_codex_exec_argv` :548, `headless_driver/argv.rs`.
- Cursor `list_models` ignores the spec-resolved binary (`tddy-coder/src/headless_from_spec.rs:105`).

## State B

- One generic headless backend runs every headless agent; per-agent behaviour is spec data plus a named stream adapter.
- A YAML-only headless agent naming an existing adapter runs a workflow turn with no Rust change.
- No headless wait is unbounded; concurrent backends cancel only their own process group.

## Delta

### New
- `packages/tddy-agent-backend/src/backend/spec_headless.rs` (generic backend; name settled at red) and a shared spawn/cancel core extracted from `headless_driver.rs`
- `packages/tddy-agent-backend/tests/spec_headless_backend_acceptance.rs`
### Modified
- `tddy-agent-backend`: `adapters/mod.rs` (`StreamAdapter` hooks, transcript factory), the four adapters, `headless_driver.rs`, `backend/mod.rs` (`AnyBackend` arms hold the generic backend), `model_catalog.rs`, `routed.rs`
- `tddy-coding-agent-specs`: `spec.rs` (`HeadlessSection`), `headless.rs`, `specs/{claude,cursor,codex,oz}.yaml`
- `tddy-coder`: `run.rs` (`create_backend` arms), `headless_from_spec.rs`
- `tddy-session-lifecycle`: `coding_agent_launcher.rs` (`headless_plan`, `headless_plan_from_spec`)
- `tddy-coding-subagent/src/session.rs`, `tddy-session-agents/src/coding_agent.rs`
- Tests moved onto the renderer: `tddy-integration-tests/tests/{backend_args,cursor_backend,codex_backend,conversation_output,tool_session_provider_route}.rs`, `tddy-agent-backend/tests/{headless_driver_acceptance,headless_renderer_port}.rs`
- Docs at wrap: `packages/tddy-agent-backend/docs/architecture.md`, `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md` (headless fields), `docs/ft/coder/1-OVERVIEW.md`
### Removed
- `ClaudeCodeBackend`, `CursorBackend`, `CodexBackend` turn paths: the three `run_child`/`invoke_sync`/`invoke_from_spec` copies, `build_claude_args` (and its `lib.rs:36` re-export), `ClaudeInvokeConfig`, `PermissionMode`, `create_mcp_config_temp_file`, `build_cursor_cli_args`, `register_cursor_mcp_config`, `build_codex_exec_argv`; the type names survive only if `spawn_oauth_login` needs one (decide at red)
- `headless_driver/argv.rs` and the `renderer: None` branch; the `HeadlessCommand` fields used only by it
- `HeadlessHarness` (replaced by the adapter's transcript factory)
- The global `CHILD_PID` slot (`set_child_pid`, `clear_child_pid`, `kill_child_process`) once no caller remains
- `tddy-coder/src/claude_from_spec.rs` (folded into `headless_from_spec.rs`)
- Launcher: `headless_program`, `mcp_servers_by_name`, `write_cursor_project_mcp_config`, `OZ_BINARY`; `mcp_servers_wrapped` if no agentless test needs it
- `TODO(claude-headless)` and `TODO(headless-others)` markers at the sites above

## Testing Strategy

### Acceptance tests

**`tddy-agent-backend`: `packages/tddy-agent-backend/tests/spec_headless_backend_acceptance.rs`** (stub `#!/bin/sh` CLIs echoing argv to `argv.log` and printing a recorded stream)

- [ ] `a_headless_agent_declared_only_in_yaml_runs_a_workflow_turn_through_its_named_stream_adapter`
- [ ] `the_claude_spec_turn_has_the_argv_mcp_config_and_session_id_the_claude_backend_produced`
- [ ] `the_cursor_spec_turn_prepends_the_system_prompt_and_forces_trust_as_the_cursor_backend_did`
- [ ] `the_codex_spec_turn_runs_read_only_in_plan_mode_and_reports_the_auth_remediation_on_a_login_failure`
- [ ] `a_turn_that_never_ends_is_cancelled_at_its_deadline_and_its_whole_process_group_is_killed`
- [ ] `two_concurrent_turns_cancel_only_their_own_process`
- [ ] `a_spec_without_a_route_section_refuses_a_provider_route`

**`tddy-coder`: `packages/tddy-coder/tests/headless_cursor_codex_from_spec_acceptance.rs`** (extend)

- [ ] `cursor_list_models_runs_the_spec_resolved_binary`

### Unit / integration tests

- [ ] `tddy-coding-agent-specs`: each new `headless` field parses and refuses an unknown value naming file and word; the four built-ins declare the values that reproduce today's behaviour (golden argv unchanged).
- [ ] `tddy-agent-backend`: goal hints → params table per spec; route-replaces-model per spec; `failure_detail` per adapter; `name()` is the spec id.
- [ ] Existing goldens (`headless_render_golden.rs`, `headless_others_render_golden.rs`) stay green unchanged.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Parity with each bespoke backend | Integration with stub CLIs | the pins are argv, files and error text |
| Deadlines and process groups | Integration with a sleeping stub | the stack rule is no hangs |
| Spec fields | Unit | pure parsing |

## Decisions & Trade-offs

- **Developer decision: "make headless generic too"** — over Exploration 1's recommendation to keep the bespoke backends behind spec-named adapters.
- Behaviour changes to confirm at red, each recorded rather than defaulted: codex gains `remote.env_pairs()` and progress; one `stderr` rule for the response; cursor's `exit_ok` checked uniformly; spec-written MCP files are cleaned up with the turn (or not); `.cursor/mcp.json` in the worktree is git-excluded as the launcher does.
- Open: the `TDDY_*` environment (see **Boundaries**).

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-headless-driver-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
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
