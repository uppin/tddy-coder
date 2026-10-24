# 2026-10-24 — The tddy-tools-only MCP argv and the agent mint are shared
**Type:** Fix

`agent_argv::tddy_tools_only_mcp_args` writes a session's `claude-mcp-config.json` registering
`tddy-tools --mcp` (with its log file) and returns `--mcp-config` + `--strict-mcp-config`;
`split_claude_extra_args` uses it, and so does the project-home main's launch in
`tddy-agent-launch`. `SplitSessions::agent_session_token_for` is `pub`, for the main's credential.
`verified_caller` refuses as "cannot sign for the caller: …", since it backs every agent mint and the
room poller, not only split sessions. The split argv is unchanged. See
[module layout](../module-layout.md).
