# 2026-08-13 — cursor-cli cannot enforce managed-codebase mode

**Category:** Future enhancement
**Source:** remote-managed-worktree changeset, 2026-08-13

`cursor-agent` has no `--allowedTools` / `--disallowedTools` equivalent anywhere in this codebase, so a
managed-codebase cursor session can only be *guided* — via `REMOTE_APPENDIX` and a
`.cursor/rules/*.mdc` entry — never *prevented* from attempting native filesystem access. claude-cli
gets hard enforcement through `build_claude_allowlist` + `--disallowedTools`.

This is why split placement (`codebase_daemon_instance_id`) is restricted to `claude-cli` in v1. Adding
cursor-cli would additionally require, all of which the non-sandboxed cursor path lacks today:

- a read-only context dir as cwd (`prepare_context_dir_with_subagent` + `copy_dir_all`) instead of the
  worktree — `cursor_cli_spawn.rs:302` discards `managed_codebase` outright (`let _ = (…)`);
- an MCP registration (`write_cursor_mcp_config`, `packages/tddy-sandbox-recipes/src/cursor_cli.rs:213`)
  written to the cursor `$HOME` or cwd — this path writes none;
- `--force --trust --approve-mcps` in argv, which `write_cursor_mcp_config` deliberately does not inject;
- tool-relay env in `session_env`, **and** on resume — `resume_cursor_cli_session` passes
  `Vec::new()` (`cli_session_manager.rs:346-364`), so any start-time env is silently lost.

Worth revisiting if cursor-agent gains a tool-allowlist or MCP-only mode.
