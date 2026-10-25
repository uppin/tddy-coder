# 2026-10-24 — A coding agent can be a session subagent

A session's roster can hold **coding agents** — Claude Code (`claude-agent-acp`), Codex (`codex-acp`)
and OpenCode (`opencode acp`) — beside model definitions. `ListSubagents` offers every ACP harness
profile and ACP `allowed_agents` entry as a coding-agent row; the main agent opens conversations with
one through the existing `subagent_*` tools. The daemon starts the agent as the session's OS user on
the session's worktree, on the profile's provider-account route, and in a managed session withdraws
its native tools and hands it `tddy-tools --mcp`; a harness that cannot be confined (`codex-acp`) is
refused there. Permission requests are denied and recorded for now. Pickers and the Agents tab badge
coding-agent rows. On a supervised host the agent is started through `tddy-supervisor`'s relayed
spawn, which the operator enables per harness binary with `allowed_relay_tool_paths`.
See [Session agent roster § Coding agents](../session-agent-roster.md#coding-agents).
