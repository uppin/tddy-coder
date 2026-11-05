# 2026-11-05 — What #821 deleted that its changeset did not list

**Category:** Deferred from `#agent-tools` 15/19 (#821, headless-driver)
**Source:** the green phase of `docs/dev/1-WIP/2026-11-04-agent-tools-headless-driver.md`

Each is gone because removing the bespoke backends, `HeadlessHarness` or the no-`TDDY_*` rule left it
with no caller or no meaning. They are listed so a reviewer can disagree, each with where it went.

| Deleted | Why | Where it went |
|---|---|---|
| `which_binary`, `tddy_tools_path` (`backend/claude.rs`), listed under *Boundaries* as kept | `pub(crate)` helpers whose only callers were the deleted `run_child`s and `create_mcp_config_temp_file`; the spec's resolved `tddy-tools` replaced `tddy_tools_path` | nothing: the spec-resolved binary |
| `splice_route_args` (`backend/routed.rs`) | the hand-built argv's route splice; `RouteModel::model_for` plus the `route_args` list replaced it | `RouteModel`, `HeadlessTurn::route_args` |
| `CodexBackend` and its `DEFAULT_CLI_BINARY` | decided at red as "only if `spawn_oauth_login` needs one": it does not | `spawn_codex_oauth_login(binary, session_dir)` and `CODEX_DEFAULT_CLI_BINARY` |
| `HeadlessCommand::{program, mcp_config}` | read only by the hand-built path (`argv.rs`, the launcher's hand-built plan) | the renderer names the program; the MCP slot writes the config |
| `tddy-tools submit` / `ask` / `transition` succeeding with no session | the stack's "no production fallback" rule; `submit` also persisted a `branch-review` `review.md` locally when `TDDY_SESSION_DIR` was set and no socket was | the error naming `--socket`, the pointer and `TDDY_SOCKET`; `persist_review_md_to_session_dir` stays as a library function |
| `kill_child_process`, `set_child_pid`, `clear_child_pid`, `get_child_pid` | the global slot the tests and the TUI shared | `kill_running_turns`, `RunningGroup` |
| the launcher's `headless_tests.rs` cases that read the hand-written MCP config at plan time (claude, oz) or the worktree `.cursor/mcp.json` (cursor), and `codex_and_oz_run_by_name_from_the_path` | they pinned files and a program name the hand-built plan produced; a spec-planned conversation writes its registration at its first turn | `tests/headless_claude_subagent_from_spec_acceptance.rs`, `tests/headless_subagents_from_spec_acceptance.rs` |
| `BackendError::InvocationFailed`'s `"backend invocation failed: "` display frame | the contract tests pin a turn's failure text without it | `"<agent> exited with code <n>: <detail>"` |
