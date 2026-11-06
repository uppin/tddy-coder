# 2026-11-03 — Headless Cursor, Codex and Oz run from their tool definitions

**Type:** Feature

`#agent-tools` 9/12 ([#798](https://github.com/uppin/tddy-coder/pull/798)). Packages:
`tddy-coding-agent-specs`, `tddy-agent-backend`, `tddy-coding-subagent`, `tddy-session-agents`,
`tddy-session-lifecycle`, `tddy-coder`.

- `specs/codex.yaml` and `specs/oz.yaml` (new; `warp.yaml` untouched), and a `coding_agent.headless`
  section in `cursor.yaml`. MCP slot kinds `Inline`, `Argv` and `Workdir`
  (`slots::headless_mcp`); `RenderedSlots.written`.
- Stream adapters `cursor-stream`, `codex-jsonl`, `oz-ndjson` and the models adapter
  `cursor-models`, each wrapping today's parser. `CursorBackend` / `CodexBackend`
  `with_headless_renderer`; `tddy_coder::headless_from_spec::{cursor_backend_from_spec,
  codex_backend_from_spec}`; `HeadlessDriver` renders all four harnesses from a renderer.
- `tddy_coding_subagent::launcher::{HEADLESS_WRITTEN_FILES, record_written_files,
  remove_recorded_files, close_headless_conversation}`; `sweep_headless_dirs` and
  `AgentRoster::conversation_closed` remove recorded files. The launcher plans every headless
  harness through `headless_plan_from_spec` and records what it will write.
- **Resolved:** `2026-10-25-cursor-project-mcp-config-outlives-its-conversation` (todo deleted).

## Decisions

- **`cursor.yaml`'s `models` section edited** (`list_command: ["{binary}", --list-models]`,
  `adapter: cursor-models`), a section #796 owns — approved by the developer 2026-11-03.
- **`coding_agents_error` refuses Claude, Cursor, Codex and Oz**, no hand-built fallback
  (contract / [G]).
- **Two parent exact-list tests widened**: n6's `the_builtin_registry_lists_claude_stream_json`
  and n8's builtin launch-adapters list, for the four new adapters (contract / [G]).
- **`headless_cursor_codex_from_spec_acceptance` prompt expectation is three argv lines**
  (approved at [G]).
- Oz is its own `oz.yaml`; Cursor's headless MCP is `headless.mcp: { kind: workdir }`; agent-specific
  argv bits are turn `params`; Codex's `-m` follows `--json`; `HeadlessHarness` stays (#801 collapses
  it); no `codex-oauth` adapter, since this path has no login retry (decided 2026-10-31).

## Deferred

- **Deleting the hand-built builders awaits approval**: `headless_driver/argv.rs`,
  `build_cursor_cli_args`, `register_cursor_mcp_config`, `build_codex_exec_argv`, and the launcher's
  `write_cursor_project_mcp_config`, `mcp_servers_by_name`, `headless_program`, `OZ_BINARY`
  ([todo](../todo/2026-11-03-headless-others-legacy-argv-deletion.md)).
- **Restructuring deferred to the `#agent-tools` sweep node** (developer). Code issues: new
  `oversized-file-codex` (479 → 606); regressed `oversized-file-cursor` (519 → 634),
  `oversized-file-headless-driver` (547 → 563), `oversized-file-coding-agent-launcher`
  (967 → 1036).
- Native-tool withdrawal for Cursor, Codex and Oz stays unproven (`tool_deny: unsupported`).

Product docs: [changelog](../../ft/coder/changelog/2026-11-03-headless-cursor-codex-oz-from-specs.md),
[Managed codebase subagents](../../ft/coder/managed-codebase-subagents.md),
[Session agent roster](../../ft/daemon/session-agent-roster.md).
