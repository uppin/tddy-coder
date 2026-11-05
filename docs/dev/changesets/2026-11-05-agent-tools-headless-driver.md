# 2026-11-05 — One spec-configured headless driver runs claude, codex and cursor turns

**Type:** Refactor

`#agent-tools` 15/19 ([#821](https://github.com/uppin/tddy-coder/pull/821)), branch
`feature/agent-tools/headless-driver`, based on `feature/agent-tools/cli-spec-features`
([#820](https://github.com/uppin/tddy-coder/pull/820)); blocks `feature/agent-tools/spec-backend` (n16).
Packages: `tddy-agent-backend`, `tddy-coding-agent-specs`, `tddy-coder`, `tddy-tools`,
`tddy-tools-cli`, `tddy-tool-engine`, `tddy-session-lifecycle`, `tddy-coding-subagent`,
`tddy-session-agents`, `tddy-tui`, `tddy-integration-tests`.

The three bespoke headless workflow backends (`ClaudeCodeBackend`, `CursorBackend`, `CodexBackend`,
about 2,500 lines with three ~90%-identical `run_child` copies) are deleted. One generic
`SpecHeadlessBackend` runs every headless agent from its spec; per-agent behaviour is spec data plus
a named stream adapter, so a YAML-only headless agent naming a registered adapter runs a workflow
turn with no Rust change. Behaviour changes, each decided and pinned by a test: no `TDDY_*` variable
in a headless agent's environment (session discovery through a pointer file), one stderr rule, one
failure frame, MCP files removed after the turn, per-turn process groups with a deadline in place of
the global `CHILD_PID` slot, and Codex turns gaining progress and the remote environment.

- `tddy-agent-backend`: `SpecHeadlessBackend` (`backend/spec_headless.rs`: `new(renderer, protocol,
  agent)`, `with_turn_deadline`, `with_model_listing`, `with_progress`, `with_provider_route`,
  `protocol()`; `DEFAULT_TURN_DEADLINE` 4 h, `MODEL_LISTING_DEADLINE` 30 s); the protocol types in
  `adapters::headless` (`HeadlessProtocol`, `SystemPromptDelivery`, `GoalParams`, `RouteModel`,
  `FreshSession`, `ScratchDir`, `EnvTemplate` + `render_env`, `HeadlessModelListing`);
  `StreamAdapter::{failure_detail, transcript}` with `TurnTranscript`, `TurnEnding`, `TurnFinished`
  (the closed `HeadlessHarness` enum and `turn_format` are gone); `backend/turn_process.rs` (process
  groups, `STDERR_GRACE`, `kill_running_turns`, `RunningGroup`); `session_pointer` (`SessionPointer`,
  `SESSION_POINTER_FILE` = `.tddy/session.json`, `find_session_pointer`) and `turn_files`
  (`TurnFiles`, `exclude_from_git`). `HeadlessDriver::new(command)` renders every conversation turn
  from the spec and gains `HeadlessCommand::turn_deadline`; `headless_driver/argv.rs` is deleted.
  `AnyBackend::{Claude, Cursor, Codex}` hold the generic backend. `backend/codex.rs` keeps only the
  sign-in (`spawn_codex_oauth_login`, `CODEX_DEFAULT_CLI_BINARY`).
- `tddy-coding-agent-specs`: seven `coding_agent.headless` protocol fields (`system_prompt`,
  `goal_params`, `approval_tool`, `workflow_mcp`, `route_model`, `fresh_session`, `scratch`),
  `HeadlessSection::protocol()`, `SpecHeadlessRenderer::{protocol, model_listing}`, declared in
  `claude.yaml`, `cursor.yaml`, `codex.yaml` (with the templated `env`: `BROWSER: "{self_exe}"`,
  `TDDY_CODEX_OAUTH_OUT`) and `oz.yaml`. The render goldens are unchanged.
- `tddy-coder`: `headless_from_spec::headless_backend_from_spec(agent, tools_dir, host)` builds the
  `claude`, `cursor` and `codex` backends; `claude_from_spec.rs` is folded into it.
- `tddy-tools`: `session_source` (`resolve_session`, `resolve_current_session`, `adopt_session`,
  `ResolvedSession`, `SessionSource`, `SessionNotFound`) — `--socket`, then the nearest
  `.tddy/session.json`, then `TDDY_SOCKET`; `submit`, `ask` and `transition` with no session fail
  naming all three. `tddy-tools-cli` adopts the resolved session before it builds its runtime.
- `tddy-tool-engine`: an exec-tool command starts with no `TDDY_*` variable, inherited or passed.
- `tddy-session-lifecycle`: the launcher plans every headless conversation from its spec
  (`headless_plan_from_spec`); `headless_program`, `mcp_servers_by_name`,
  `write_cursor_project_mcp_config` and `OZ_BINARY` are deleted, and the worktree file's
  git-exclusion goes through `turn_files::exclude_from_git`.
- `tddy-coding-subagent`, `tddy-session-agents`: `start_headless(command, model)` (no harness).
- `tddy-tui`: Ctrl+C calls `kill_running_turns`.

Permanent docs: [tddy-agent-backend architecture](../../../packages/tddy-agent-backend/docs/architecture.md),
[coding agent specs § The headless protocol](../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md#the-headless-protocol),
[tddy-tools session resolution](../../../packages/tddy-tools/docs/session-resolution.md),
[coding-agent launcher](../../../packages/tddy-session-lifecycle/docs/coding-agent-launcher.md),
[Coder overview](../../ft/coder/1-OVERVIEW.md).

## Deletions beyond the plan — approved

The user approved every deletion the green phase made beyond the changeset's planned list on
2026-11-05. The final list:

| Deleted | Why | Where it went |
|---|---|---|
| `which_binary`, `tddy_tools_path` (`backend/claude.rs`), planned as kept | `pub(crate)` helpers whose only callers were the deleted `run_child`s and `create_mcp_config_temp_file` | the spec-resolved binary |
| `splice_route_args` (`backend/routed.rs`) | the hand-built argv's route splice | `RouteModel::model_for` and `HeadlessTurn::route_args` |
| `CodexBackend` and its `DEFAULT_CLI_BINARY` | the sign-in needs neither | `spawn_codex_oauth_login(binary, session_dir)` and `CODEX_DEFAULT_CLI_BINARY` |
| `HeadlessCommand::{program, mcp_config}` | read only by the hand-built path | the renderer names the program; the MCP slot writes the config |
| `tddy-tools submit` / `ask` / `transition` succeeding with no session (`submit` also wrote a `branch-review` `review.md` into `TDDY_SESSION_DIR` with no socket) | the stack's no-production-fallback rule | the error naming `--socket`, the pointer and `TDDY_SOCKET`; `persist_review_md_to_session_dir` stays a library function |
| `kill_child_process`, `set_child_pid`, `clear_child_pid`, `get_child_pid` | the global slot the tests and the TUI shared | `kill_running_turns`, `RunningGroup` |
| the launcher's `headless_tests.rs` cases that read the hand-written MCP config at plan time (claude, oz) or the worktree `.cursor/mcp.json` (cursor), and `codex_and_oz_run_by_name_from_the_path` | they pinned files and a program name the hand-built plan produced | `tests/headless_claude_subagent_from_spec_acceptance.rs`, `tests/headless_subagents_from_spec_acceptance.rs` |
| `BackendError::InvocationFailed`'s `"backend invocation failed: "` display frame | the contract tests pin a turn's failure text without it | `"<agent> exited with code <n>: <detail>"` |

## Backlog

Resolved and deleted:

- `2026-11-02-claude-hand-built-headless-paths` — *Claude's hand-built headless paths outlive the
  spec path*: `build_claude_args`, `create_mcp_config_temp_file`, `argv.rs::claude()` and the
  launcher's hand-built plan are gone.
- `2026-11-03-headless-others-legacy-argv-deletion` — *Hand-built Cursor, Codex and Oz argv builders
  outlive the spec path*: `headless_driver/argv.rs`, the renderer-less branch, `build_cursor_cli_args`,
  `register_cursor_mcp_config`, `build_codex_exec_argv`, and the launcher's
  `write_cursor_project_mcp_config`, `mcp_servers_by_name`, `headless_program`, `OZ_BINARY`.
- `2026-11-05-headless-driver-unplanned-deletions` — *What #821 deleted that its changeset did not
  list*: approved by the user on 2026-11-05; its table is the one above.

Deferred, kept open:

- `2026-11-05-codex-oauth-out-env-in-headless-agent` — Codex's `TDDY_CODEX_OAUTH_OUT`, the one
  `TDDY_*` variable a headless agent still sees (the Codex OAuth follow-up stack).
- `2026-11-05-headless-conversation-tool-server-env` — a Codex conversation's tool-server `TDDY_*`
  variables still ride `HeadlessCommand::env`.
- `2026-11-05-session-pointer-lifecycle-hazards` — one pointer slot per worktree, and stale pointers.

Referenced, unchanged: `2026-10-25-headless-cursor-codex-oz-cannot-withdraw-native-tools`.

## Code issues (`packages/tddy-agent-backend/docs/code-issues/`)

Closed by deletion — the files and functions no longer exist, or no longer hold the code measured:

| Record | Last measurement (before #821) | After |
|---|---|---|
| `complexity-claude-run-child` | `ClaudeCodeBackend::run_child`: 290 lines (fn line to closing brace), budget 60 (2026-11-02, #795) | the function is deleted |
| `oversized-file-claude` | `backend/claude.rs`: 784 production lines, budget 500 (2026-11-02, #795) | 7 lines: the module holds only the re-export of `claude/transcript_usage.rs` |
| `oversized-file-cursor` | `backend/cursor.rs`: 634 production lines (2026-11-03, #798) | the file is deleted |
| `oversized-file-codex` | `backend/codex.rs`: 606 production lines (2026-11-03, #798) | 140 production lines: the sign-in only |
| `oversized-file-headless-driver` | `backend/headless_driver.rs`: 563 production lines (2026-11-03, #798) | 477 at the green commit — **then reopened**: the conversation turn deadline took it to 509 (> 500), so the record is kept, `Open — regressed 2026-11-05` |

Open records added (restructuring is the stack's sweep node's): `oversized-file-spec-headless`
(`spec_headless.rs`, 645 lines), `complexity-spec-headless-read-turn` (`read_turn`, 141 lines),
`complexity-spec-headless-invoke` (`invoke` 103 / `turn_for` 67 lines). Regressed:
`complexity-headless-driver-run-turn` (`run_turn` 91, `read_turn` 83 lines). Partly fixed:
`oversized-file-backend-mod` 851 → 815 production lines (seam A, the global child-process slot,
closed).
