# Changeset: One headless driver, configured by the spec, replaces the Claude, Codex and Cursor workflow backends

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Refactor (behaviour-preserving, with named, decided behaviour changes)
**Complexity**: 5 (re-scored at red, unchanged band, larger inside it) — row-4 signals: crates `tddy-agent-backend`, `tddy-coding-agent-specs` (7 new `headless` fields in 4 YAMLs), `tddy-coder`, `tddy-session-lifecycle`, `tddy-integration-tests`, and now `tddy-tools` (+ `tddy-tools-cli` tests): the no-`TDDY_*` rule moved session discovery into a pointer file `tddy-tools` reads; a spawned-process seam whose cancellation moves from a global pid slot to per-turn process groups with a deadline (concurrency); ~2,500 lines of three backends collapsed; byte-for-byte argv/MCP/error-text pins across ~15 suites; 49 failing contract tests across 6 suites. The spec-field set is settled (below)
**Stack:** `#agent-tools 15/19` — branch `feature/agent-tools/headless-driver`, base `feature/agent-tools/cli-spec-features` (#820)
**PR:** [#821](https://github.com/uppin/tddy-coder/pull/821)
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
- [ ] [`tddy-tools`](../../../packages/tddy-tools/README.md): `session_source` — a `tddy-tools` finds its session from `--socket`, then `.tddy/session.json` at or above its cwd, then `TDDY_SOCKET`; none is an error naming all three (added at red: the no-`TDDY_*` rule needs it, see **Decisions**)
- `tddy-tools-cli`: tests only (`session_pointer_submit_acceptance.rs`)

## Responsibility

- **One generic headless workflow backend** (`impl CodingBackend`), configured by the agent's spec: argv from `SpecHeadlessRenderer`, stdout decoded by the spec's stream adapter, async spawn in its own process group with `killpg` on cancel, a stderr grace and a **turn deadline**. It shares the spawn/cancel/stderr core with `HeadlessDriver` (the conversation driver) instead of a fourth copy.
- **Protocol differences become spec fields**, each named after what it does, never after an agent — **settled at red** as `coding_agent.headless.{system_prompt: flag|prepend, goal_params: {always, plan_mode, read_only_plan, accept_edits}, approval_tool, workflow_mcp, route_model: {kind: kept|replaced|replaced_if_route_has, flag}, fresh_session: caller|agent, scratch: session_dir|workdir}`; templated `headless.env` values `{session_dir}`, `{self_exe}` (`render_env`). The types (`HeadlessProtocol` and its parts) live beside the port in `tddy_agent_backend::adapters::headless` and are re-exported by `tddy_coding_agent_specs::headless`. `claude.yaml`, `cursor.yaml`, `codex.yaml`, `oz.yaml` declare them.
- **The session pointer**: before each turn the backend writes `<workdir>/.tddy/session.json` (`SessionPointer`: socket, repo dir, session dir, remote env), git-excluded, removed with the turn (`TurnFiles`); `tddy-tools` resolves its session from it (`tddy_tools::session_source`).
- **Small named stream adapters where unavoidable**: `StreamAdapter::failure_detail(stream, stderr)` (settled at red: no `keeps_stderr` — one stderr rule for every agent, below), implemented by the existing `claude-stream-json`, `cursor-stream`, `codex-jsonl`, `oz-ndjson` adapters; `turn_format() -> Option<HeadlessHarness>` becomes a transcript factory on the adapter, so the closed `HeadlessHarness` enum is deleted.
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
- The agent process sees no `TDDY_*` variable — ✅ **settled at red** (the user's rule, not relaxed). Today all three backends set `TDDY_SOCKET`, `TDDY_REPO_DIR`, `TDDY_SESSION_DIR` (and `TDDY_REMOTE_*` from `remote.env_pairs()`), and codex sets `TDDY_CODEX_OAUTH_OUT`. The generic backend strips every inherited `TDDY_*` and sets none; a `tddy-tools` the agent runs — from its shell (`tddy-tools submit`, the command every workflow prompt names) or as its MCP server, which runs in the agent's cwd — finds the session through `.tddy/session.json` (see **Responsibility**). `TDDY_SOCKET` stays a third source for the daemon's own callers and tests only. **Exception:** codex's `TDDY_CODEX_OAUTH_OUT` (OAuth capture) is kept, `TODO(headless-driver)`, owned by the Codex OAuth follow-up stack — [2026-11-05-codex-oauth-out-env-in-headless-agent.md](../todo/2026-11-05-codex-oauth-out-env-in-headless-agent.md).
- No hangs: turn deadline (`DEFAULT_TURN_DEADLINE` 4h, `with_turn_deadline`), stderr grace, `list_models` deadline (`MODEL_LISTING_DEADLINE` 30s); the global `CHILD_PID` slot is replaced by per-turn process groups. `tddy-tui`'s Ctrl+C (`kill_child_process`) is its last caller: green either kills every running turn's group from it or keeps it, `TODO(headless-driver)` — listed under **Removed** only once no caller remains.
- No fallback: a missing tool server is an error, never a silently dropped MCP config (claude.rs:465 today); no process-cwd fallback for the workdir.
- Deletions not listed in **Removed** need the developer's approval.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n13` route-support (#819) | `RoutedAgent`, agent-keyed `RouteRefusal`, route bodies by (shape, provider) | the generic backend's route is resolved from the spec's `RoutedAgent`; a spec without `route` refuses a route | add route bodies or `RouteRefusal` variants |
| `n14` cli-spec-features (#820) | (line predecessor only) | nothing — no real edge | — |
| `n13` route-support (#819) — **sequencing found at red** | `routed_agent(spec)`, `CodingAgentSpec::product`, `by_spec::agent_routable` (all `TODO(route-support)` today) | `headless_backend_from_spec` builds the backend's `RoutedAgent` with `routed_agent`, and `SpecHeadlessBackend::with_provider_route` refuses through `agent_routable`. So **every test in `tddy-coder/tests/spec_headless_backend_acceptance.rs` needs #819 green** once n15's own bodies exist; the `tddy-agent-backend` tests inject a hand-built `RoutedAgent` and do not | implement any of them |
| `n6` claude-headless (#795), `n9` headless-others (#798) | `SpecHeadlessRenderer`, `HeadlessSection`, the four stream adapters, the goldens | renders every turn through them | change the renderer's argv output for existing params |

## Draft PR contract

The contract commit publishes (unimplemented bodies are `// TODO(headless-driver): implement`):

- `tddy_agent_backend::SpecHeadlessBackend` (`backend/spec_headless.rs`): `new(renderer, protocol, agent: RoutedAgent)`, `with_turn_deadline`, `with_model_listing`, `with_progress`, `with_provider_route -> Result<Self, by_spec::RouteRefusal>`, `protocol()`, `impl CodingBackend` (`invoke`, `name` = spec id, `list_models`); `DEFAULT_TURN_DEADLINE`, `MODEL_LISTING_DEADLINE`.
- `tddy_agent_backend::adapters::headless`: `HeadlessProtocol`, `SystemPromptDelivery`, `GoalParams::bind`, `RouteModel::model_for`, `FreshSession`, `ScratchDir`, `EnvTemplate` + `render_env`, `HeadlessModelListing`.
- `StreamAdapter::failure_detail(stream, stderr)` and `StreamAdapter::transcript(updates) -> Option<Box<dyn TurnTranscript>>` with `TurnTranscript`, `TurnEnding`, `TurnFinished` (`adapters/mod.rs`), stubbed in the four adapters; `turn_format` stays until the transcript replaces it.
- `tddy_agent_backend::session_pointer` (`SessionPointer`, `SESSION_POINTER_FILE`, `find_session_pointer`) and `tddy_agent_backend::turn_files` (`TurnFiles::write`, `write_session_pointer`, drop removes; `exclude_from_git`).
- `tddy_coding_agent_specs`: `HeadlessSection`'s seven protocol fields and `HeadlessSection::protocol()`; `SpecHeadlessRenderer::protocol()` and `model_listing()`; the YAML lines in the four built-ins (codex's templated `env` is green's, see the `TODO` in `codex.yaml`).
- `tddy_coder::headless_from_spec::headless_backend_from_spec(agent, tools_dir, host)` — the changeset's `from_spec(spec, host, progress)`: `progress` is the builder `with_progress`, and the registry plus `tools_dir` stand for the spec because the closure resolves from the registry.
- `tddy_tools::session_source::{resolve_session, ResolvedSession, SessionSource, SessionNotFound}`.

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
- `packages/tddy-agent-backend/src/backend/spec_headless.rs` (`SpecHeadlessBackend`) and a shared spawn/cancel core extracted from `headless_driver.rs` (green)
- `packages/tddy-agent-backend/src/session_pointer.rs`, `packages/tddy-agent-backend/src/turn_files.rs`
- `packages/tddy-tools/src/session_source.rs`
- Tests: `tddy-agent-backend/tests/{spec_headless_backend_acceptance,headless_protocol_unit}.rs`, `tddy-coder/tests/spec_headless_backend_acceptance.rs` (the spec-parity acceptance: only `tddy-coder` names both the spec loader and the backend without a dev-dependency cycle), `tddy-coding-agent-specs/tests/headless_protocol_spec_unit.rs`, `tddy-tools/tests/session_source_unit.rs`, `tddy-tools-cli/tests/session_pointer_submit_acceptance.rs`
- `docs/dev/todo/2026-11-05-codex-oauth-out-env-in-headless-agent.md`
### Modified
- `tddy-agent-backend`: `adapters/mod.rs` (`StreamAdapter` hooks, transcript factory), the four adapters, `headless_driver.rs`, `backend/mod.rs` (`AnyBackend` arms hold the generic backend), `model_catalog.rs`, `routed.rs`
- `tddy-coding-agent-specs`: `spec.rs` (`HeadlessSection`), `headless.rs`, `specs/{claude,cursor,codex,oz}.yaml`
- `tddy-coder`: `run.rs` (`create_backend` arms), `headless_from_spec.rs`
- `tddy-tools`: `cli.rs` (`submit`, `ask` and every `TDDY_SOCKET` reader resolve through `session_source`; `--socket` added), `server.rs` (`permission_relay_socket_path` and the relays)
- **No prompt changes**: every workflow prompt keeps naming `tddy-tools submit …`, which now finds its session from the cwd — `tddy-workflow-recipes/src/{bugfix/{analyze,interview,mod},grill_me/prompt,plan_pr_stack/{hooks,prompt},pr_stack/{hooks,mod},review/{mod,persist,prompt},tdd/{acceptance_tests,demo,evaluate,green,plan_task,planning,red,refactor,update_docs,validate_subagents},tdd_small/{post_green_review,red,submit}}.rs` and the `parser/*.rs` docs, plus `tddy-workflow-engine/src/workflow/{backend_invoke_task,recipe,action_cache}.rs`, `tddy-presenter/src/presenter/presenter_impl/questions.rs`, `tddy-pr-stack/src/docs.rs`
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

All written at red, fully implemented, and failing for this node's own reason (a `TODO(headless-driver)` body). Line numbers at the contract commit.

**`tddy-agent-backend`: `packages/tddy-agent-backend/tests/spec_headless_backend_acceptance.rs`** (fixture renderer; a sleeper stub with a grandchild, a quick stub)

- [x] `a_turn_that_never_ends_is_cancelled_at_its_deadline_and_its_whole_process_group_is_killed`
- [x] `two_concurrent_turns_cancel_only_their_own_process`

**`tddy-coder`: `packages/tddy-coder/tests/spec_headless_backend_acceptance.rs`** (real specs through `headless_backend_from_spec`; stubs `claude`, `agent`, `codex`, `lark`, `tddy-tools` record argv, `TDDY_*` env, the pointer and the registration they found; the repo is a git checkout)

- [x] `a_headless_agent_declared_only_in_yaml_runs_a_workflow_turn_through_its_named_stream_adapter`
- [x] `a_spec_without_a_route_section_refuses_a_provider_route`
- [x] `the_claude_spec_turn_has_the_argv_mcp_config_and_session_id_the_claude_backend_produced`
- [x] `the_cursor_spec_turn_prepends_the_system_prompt_and_forces_trust_as_the_cursor_backend_did`
- [x] `the_codex_spec_turn_runs_read_only_in_plan_mode_and_reports_the_auth_remediation_on_a_login_failure`
- [x] `cursor_list_models_runs_the_spec_resolved_binary` (moved here from `headless_cursor_codex_from_spec_acceptance.rs`: it drives the new constructor)
- [x] `a_headless_agent_process_sees_no_tddy_variable_even_when_the_daemon_has_one` — decision: no `TDDY_*`
- [x] `a_turn_points_its_working_directory_at_the_session_for_a_shell_run_tddy_tools` — decision: the session pointer, git-excluded, removed
- [x] `the_mcp_config_a_claude_turn_wrote_is_removed_when_the_turn_ends` — decision: MCP files cleaned up
- [x] `a_cursor_turn_git_excludes_the_mcp_config_it_writes_in_the_worktree_and_removes_it_after` — decision: `.cursor/mcp.json` git-excluded
- [x] `a_codex_turn_reports_its_exit_as_progress_and_hands_its_tools_the_remote_session` — decision: codex gains progress and remote env (through the pointer; its env carries only `TDDY_CODEX_OAUTH_OUT`)
- [x] `every_agents_response_keeps_its_stderr_when_it_left_any` — decision: one stderr rule

**`tddy-tools-cli`: `packages/tddy-tools-cli/tests/session_pointer_submit_acceptance.rs`**

- [x] `a_shell_run_tddy_tools_submit_reaches_the_session_through_the_pointer_with_no_tddy_env`

### Unit / integration tests

- [x] `tddy-coding-agent-specs` (`tests/headless_protocol_spec_unit.rs`): each built-in declares the protocol its bespoke backend ran with (claude, cursor, codex, oz); a section with no protocol field gets the documented defaults; Cursor's model listing runs its spec-resolved binary through `cursor-models`; a spec with none hands the backend none. **Green at contract** (the published YAML surface, pinned so green cannot loosen it): an unknown `system_prompt`, `route_model` kind, `fresh_session`, `scratch` or `goal_params` hint is refused at load naming the file and the word, plus a built-in positive control.
- [x] `tddy-agent-backend` (`tests/headless_protocol_unit.rs`): goal hints → params per spec (7 cases) and map override order; route-replaces-model (6 cases); `render_env` renders `{session_dir}`/`{self_exe}` and refuses `{session_dir}` without one; `failure_detail` per adapter (5 cases) and the codex sign-in remediation; every adapter makes a transcript; `name()` is the spec id; turn files git-excluded then removed; the pointer found from below and removed with the turn; an unparsable pointer is an error; turn files outside a git checkout are refused.
- [x] `tddy-tools` (`tests/session_source_unit.rs`): `--socket` beats the pointer and env; the pointer beats env; env last; nothing anywhere names all three sources; an unparsable pointer is an error, never skipped.
- [ ] Existing goldens (`headless_render_golden.rs`, `headless_others_render_golden.rs`) stay green unchanged — **verified at red**: the YAML additions change no rendered argv.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Parity with each bespoke backend | Integration with stub CLIs | the pins are argv, files and error text |
| Deadlines and process groups | Integration with a sleeping stub | the stack rule is no hangs |
| Spec fields | Unit | pure parsing |

## Decisions & Trade-offs

- **Developer decision: "make headless generic too"** — over Exploration 1's recommendation to keep the bespoke backends behind spec-named adapters.
All ✅ **settled at red** (orchestrator, under the user's "don't stop"; each pinned by a test above):

- ✅ **No `TDDY_*` in a headless agent's environment** — the user's rule. Session discovery moves to `.tddy/session.json` in the agent's cwd, written per turn, git-excluded through `info/exclude`, removed when the turn ends; `tddy-tools` resolves `--socket` → pointer → `TDDY_SOCKET` (daemon callers and tests only) and fails naming all three otherwise, never a silent local run. Codex's inline MCP slot passes variable *names* only (`env_vars=[…]`); its tool server runs in the agent's cwd and finds the same pointer, so no values are needed. The MCP registrations stay byte-identical (no `env` block). Exception: `TDDY_CODEX_OAUTH_OUT`, kept with `TODO(headless-driver)` for the Codex OAuth follow-up stack.
- ✅ **Codex gains remote env and progress**: the remote env reaches codex's tools through the pointer's `remote` map, not its environment; progress (`AgentExited`) goes to the request's sink, else the backend's `with_progress` callback, for every agent.
- ✅ **One stderr rule**: a response carries the child's stderr whenever it left any (the bespoke Claude backend dropped it when the stream had lines). No `keeps_stderr` hook.
- ✅ **One failure frame**: `"<agent id> exited with code <n>: <failure_detail>"` (or without the detail when empty); Codex's `\nInvoked: …` suffix goes, its remediation stays after a blank line. A deadline is `"<agent id> turn did not end within its <d> deadline; its process group was killed"`.
- ✅ **MCP config files are cleaned up after the turn**, `.cursor/mcp.json` included; files under the worktree are git-excluded while they exist; a worktree that is not a git checkout is refused.
- ✅ **`HeadlessDriver` vs `CodingBackend`**: a new generic `SpecHeadlessBackend` shares the driver's spawn/cancel core; the global `CHILD_PID` slot is replaced by per-turn process groups under a turn deadline, a cancel (dropping the turn's future) killing that group and no other.
- ✅ Cursor's `exit_ok` is checked uniformly through its adapter (discovery's fifth change; it follows from one backend).
- ✅ `route_model` is an internally tagged map (`{ kind: replaced_if_route_has, flag: --model }`) — the house style of `McpSlot`; an externally tagged `{ replaced_if_route_has: … }` does not load through `serde_yaml`.
- ✅ Codex's templated `env` (`BROWSER: "{self_exe}"`, `TDDY_CODEX_OAUTH_OUT`) is declared at green, together with `render_env` in `HeadlessDriver`, which reads the same `env` — declaring it at red would hand codex conversations a literal `{self_exe}`.

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-headless-driver-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the user: "finish without stopping"; decisions taken by the orchestrator)
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
- `tddy-tools submit` (and `ask`) without `TDDY_SOCKET` today succeeds locally (`output_success`) — a fallback the "missing everything is an error" rule removes. `tddy-tools-cli/tests/submit_goal_routing_acceptance.rs` and parts of `cli_integration.rs` drive that path on purpose (`env_remove("TDDY_SOCKET")`); green gives them a `--socket` or a pointer, or keeps a validate-only mode explicitly — not silently.
- The launcher's `exclude_from_git` (`tddy-session-lifecycle`) duplicates `turn_files::exclude_from_git`; it moves when `write_cursor_project_mcp_config` is deleted (`TODO(headless-driver)`).
- `HeadlessDriver` (conversations) still passes its command env; it shares the core at green and must apply the same no-`TDDY_*` rule and `render_env`.
- `two_concurrent_turns_cancel_only_their_own_process` currently fails after its 20 s wait ("the agent never wrote agent.pid") because the spawned turns panic on the `todo!()` — the panic, not a hang.
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
