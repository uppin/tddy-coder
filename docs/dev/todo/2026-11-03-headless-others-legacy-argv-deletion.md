# 2026-11-03 — Hand-built Cursor, Codex and Oz argv builders outlive the spec path

**Category:** Deferred from `#agent-tools` 9/12 (#798, headless-others)
**Source:** `packages/tddy-agent-backend/src/backend/{cursor.rs,codex.rs}`; `packages/tddy-agent-backend/src/backend/headless_driver/argv.rs`; `packages/tddy-session-lifecycle/src/coding_agent_launcher.rs`; `packages/tddy-coder/src/codex_binaries.rs`

Every production path now renders Cursor, Codex and Oz from `cursor.yaml`, `codex.yaml` and
`oz.yaml`: `create_backend("cursor"|"codex")`, `HeadlessDriver` with a renderer, and the coding-agent
launcher (`headless_plan_from_spec`). The hand-built builders stay compiling because tests still
drive them, and deleting a file needs a request first. For a human to delete, once those tests are
moved onto a renderer:

- `packages/tddy-agent-backend/src/backend/headless_driver/argv.rs` (`cursor()`, `codex()`, `oz()`,
  `codex_mcp_server_overrides`) and the `renderer: None` branch of `HeadlessDriver::run_turn`
  (driven by `tests/headless_driver_acceptance.rs` and the argv unit tests);
- `build_cursor_cli_args`, `register_cursor_mcp_config` (`backend/cursor.rs`) and
  `build_codex_exec_argv` (`backend/codex.rs`), with the `headless: None` branch of
  `CursorBackend::invoke_sync` / `CodexBackend::invoke_sync`;
- in the launcher: `write_cursor_project_mcp_config`, `mcp_servers_by_name`, `headless_program`,
  `OZ_BINARY` (see the `TODO(headless-others)` markers there);
- `packages/tddy-coder/src/codex_binaries.rs`'s `resolve_codex_binary` is still used by the
  `codex-acp` backend and the OAuth relay; only its `codex` workflow use is gone.
