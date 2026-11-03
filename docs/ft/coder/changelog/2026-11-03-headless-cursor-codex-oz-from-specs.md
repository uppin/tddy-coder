# 2026-11-03 — Headless Cursor, Codex and Oz run from their tool definitions

`#agent-tools` 9/12 ([#798](https://github.com/uppin/tddy-coder/pull/798)).

- **`tddy-coder --agent cursor` and `--agent codex`** render each invocation from `cursor.yaml`'s and
  the new `codex.yaml`'s `coding_agent.headless` sections, with the same argv as before, including
  resume and the Codex thread id. A Codex sign-in failure still carries the remediation hint.
- **Headless Cursor, Codex and Oz subagents** launch from their specs (Oz from its own `oz.yaml`,
  not `warp.yaml`). MCP is registered as each spec declares it: a worktree `.cursor/mcp.json`,
  inline `-c mcp_servers.*` overrides, or an `--mcp <file>`.
- **A closed Cursor conversation leaves no `.cursor/mcp.json`**: the files a conversation's plan
  writes are recorded, and its close and the daemon's start-up sweep remove them. A retry under the
  same conversation id is admitted.
- When the daemon's coding agents do not load, a headless Claude, Cursor, Codex or Oz launch is
  refused with the load error.

See [Managed codebase subagents](../managed-codebase-subagents.md) and
[Session agent roster](../../daemon/session-agent-roster.md).
