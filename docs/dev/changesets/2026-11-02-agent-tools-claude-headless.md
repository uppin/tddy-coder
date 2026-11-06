# 2026-11-02 — Headless Claude runs from its tool definition through a named stream adapter

**Type:** Feature

`#agent-tools 6/12` ([#795](https://github.com/uppin/tddy-coder/pull/795)), base `feature/agent-tools/claude-jail` ([#794](https://github.com/uppin/tddy-coder/pull/794)). Packages: `tddy-coding-agent-specs`, `tddy-agent-backend`, `tddy-session-lifecycle`, `tddy-coder`.

## State A

- `backend/claude.rs` (`build_claude_args`, `create_mcp_config_temp_file`) and `headless_driver/argv.rs::claude()` each hand-built `claude -p … --output-format stream-json --verbose`; the launcher hand-built the subagent's MCP file and approval tool.
- `GoalHints::claude_nonzero_exit_ok_if_structured_response` decided a non-zero exit, so only plan tolerated one.

## State B

- `claude.yaml` gains `coding_agent.headless` (`HeadlessSection { argv, env, adapter, exit_ok_if_structured, mcp }`): argv with `permission_mode`, `model`, `session_id`/`resume`, `{ splice: slots }`, `{ splice: route_args }`; MCP via `--mcp-config` without `--strict-mcp-config`; `adapter: claude-stream-json`; `exit_ok_if_structured: true` — every Claude goal now tolerates a non-zero exit with a structured result.
- `tddy_agent_backend::adapters`: `StreamAdapter`, `AdapterRegistry::builtin()` with `claude-stream-json` (wraps the unchanged parser), and the `HeadlessRenderer` port the backends render through. `tddy_coding_agent_specs::headless::SpecHeadlessRenderer` implements it; spec load refuses an unregistered adapter, naming it.
- `ClaudeCodeBackend::with_headless_renderer` and `HeadlessCommand.{renderer, scratch_dir}` render each invocation. The MCP registration moves to `<session_dir>/claude-mcp-config.json` (backend) and `<tddyhome>/headless/<conversation>/claude-mcp-config.json` (driver), written 0600. A turn with no `session_dir`/`scratch_dir` is refused.
- `tddy-coder`: `create_backend("claude")` builds `claude_from_spec::claude_backend_from_spec(<tddyhome>/tools, ProcessToolHost)`; it takes a `tools_dir` parameter.
- `tddy-session-lifecycle`: `DaemonCodingAgentLauncher::with_coding_agents`; `for_daemon` installs the host's coding agents, and a load failure refuses headless `claude` launches (no fallback). `tddy-coding-agent-specs` is a normal dependency.
- Argv is byte-identical to the hand-built one, pinned by `tests/headless_render_golden.rs` and `claude_headless_from_spec_acceptance`.

## Deferred

- The hand-built paths stay while callers remain: `build_claude_args` (`tddy-integration-tests/tests/backend_args.rs`), `create_mcp_config_temp_file` (renderer-less `ClaudeCodeBackend`), `argv.rs::claude()` (`tests/headless_driver_acceptance.rs`), the launcher's hand-built Claude MCP registration (launchers without coding agents). See `docs/dev/todo/2026-11-02-claude-hand-built-headless-paths.md`.
- File-size and complexity growth in `claude.rs`, `headless_driver.rs`, `coding_agent_launcher.rs` and `run.rs` is recorded in the packages' `docs/code-issues/`, deferred to the `#agent-tools` sweep node.

## Fix (2026-11-03, /fix-pr)

CI on Linux failed `run::agent_def_handover_tests::the_default_claude_backend_is_still_selectable_by_name`: `create_backend("claude")` resolves `claude` and its `tddy-tools` dep at creation, and the runner had no `tddy-tools`. It passed only on hosts where one is installed. `create_backend` now takes the `ToolHost` it resolves against (production passes `ProcessToolHost`), and the test passes a host whose `PATH` holds stub `claude`/`tddy-tools` only. The assertion is unchanged. Eager resolution is kept: with no `tddy-tools`, a `claude` backend is refused rather than run without its MCP server.
