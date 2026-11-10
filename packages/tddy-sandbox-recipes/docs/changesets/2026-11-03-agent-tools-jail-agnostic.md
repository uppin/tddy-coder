# 2026-11-03 — Recipes name no coding agent

**Type:** Feature

`#agent-tools` 10/12 ([#799](https://github.com/uppin/tddy-coder/pull/799)).

- Removed: `SandboxRecipe::{ClaudeCli, CursorCli, OpenCodeCli, MuseCli}`, `detect_recipe_from_argv`, `claude_runner_env_overlay`, `prepare_persistent_claude_home`, and the Claude/Cursor MCP and credential writers (`claude_cli.rs`, `cursor_cli.rs`, `opencode_cli.rs`, `muse_cli.rs`).
- The Claude tool lists moved to `tddy_coding_agent_specs::claude_tools`.

See [Sandbox builder](../../../../docs/ft/coder/sandbox-builder.md).
