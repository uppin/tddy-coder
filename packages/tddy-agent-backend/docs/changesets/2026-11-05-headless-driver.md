# 2026-11-05 — One spec-configured headless backend replaces the Claude, Cursor and Codex backends

**Type:** Refactor

`#agent-tools` 15/19 ([#821](https://github.com/uppin/tddy-coder/pull/821)).

- `SpecHeadlessBackend` (`backend/spec_headless.rs`): one headless workflow `CodingBackend` per agent spec — argv from the spec renderer, stdout through the spec's stream adapter, the `coding_agent.headless` protocol (`HeadlessProtocol` and its parts in `adapters::headless`, `render_env` for `{session_dir}` / `{self_exe}`), a turn deadline (`DEFAULT_TURN_DEADLINE` 4 h), `STDERR_GRACE`, `list_models` from the spec's listing under `MODEL_LISTING_DEADLINE` (30 s). No `TDDY_*` variable reaches the agent but Codex's `TDDY_CODEX_OAUTH_OUT`; a turn needs a working directory that is a git checkout.
- `StreamAdapter::{failure_detail, transcript}` (`TurnTranscript`, `TurnEnding`, `TurnFinished`); `HeadlessHarness` and `turn_format` deleted.
- `backend/turn_process.rs`: per-turn process groups, `kill_running_turns`, `RunningGroup`; the global `CHILD_PID` slot and `kill_child_process` / `set_child_pid` / `clear_child_pid` / `get_child_pid` deleted.
- `session_pointer` (`.tddy/session.json`, `SessionPointer`, `find_session_pointer`) and `turn_files` (`TurnFiles`, `exclude_from_git`): a turn's files are git-excluded and removed when it ends.
- `HeadlessDriver::new(command)`: every conversation turn rendered from the spec, `HeadlessCommand::turn_deadline`; `headless_driver/argv.rs` and `HeadlessCommand::{program, mcp_config}` deleted.
- Deleted: `ClaudeCodeBackend`, `CursorBackend`, `CodexBackend` (`backend/cursor.rs`; `claude.rs` keeps the transcript-usage re-export, `codex.rs` the sign-in: `spawn_codex_oauth_login`, `CODEX_DEFAULT_CLI_BINARY`), `build_claude_args`, `ClaudeInvokeConfig`, `PermissionMode`, `create_mcp_config_temp_file`, `build_cursor_cli_args`, `register_cursor_mcp_config`, `build_codex_exec_argv`, `which_binary`, `tddy_tools_path`, `splice_route_args`, and `InvocationFailed`'s `backend invocation failed: ` frame.
- Code issues: `complexity-claude-run-child`, `oversized-file-claude`, `oversized-file-cursor`, `oversized-file-codex` closed by deletion; `oversized-file-headless-driver` 563 → 477 → 509, kept open (regressed); added `oversized-file-spec-headless`, `complexity-spec-headless-read-turn`, `complexity-spec-headless-invoke`; `complexity-headless-driver-run-turn` regressed; `oversized-file-backend-mod` 851 → 815.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md).
